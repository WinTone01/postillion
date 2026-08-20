-- Postillion — Supabase şeması.
--
-- Sohbet eşitlemesi bir CRDT güncelleme kaydı. Sunucunun birleştirme yapması
-- GEREKMİYOR: loro güncellemeleri sıra bağımsız birleşiyor ve anlık görüntüyü
-- istemci üretiyor. O yüzden buradaki tablolar yalnızca "ekle ve oku" —
-- iş mantığı yok, tetikleyici yok.
--
-- Cloudflare'deki ChatRoom da tam olarak bunu yapıyordu (kaynağında tek bir
-- LoroDoc yok, saf röle); bu şema onun kalıcı hâli.

-- ── sohbet satır kaydı ──────────────────────────────────────────────────────

create table if not exists chat_rows (
  -- `seq` GLOBAL artan: sohbet başına sayaç tutmak her eklemede kilit
  -- gerektirirdi. Protokolün istediği tek şey sohbet İÇİNDE artan olması ve
  -- global kimlik bunu zaten sağlıyor.
  seq        bigint generated always as identity primary key,
  chat_id    text        not null,
  owner      uuid        not null default auth.uid(),
  -- Yazan cihaz: istemci kendi satırlarını geri almamak için süzüyor
  -- (`excludeOwn`).
  device     text        not null,
  -- Aynı gönderimin parçaları; ack bununla eşleşiyor.
  batch_id   text        not null,
  -- Opak loro güncellemesi. Sunucu içine bakmıyor.
  payload    bytea       not null,
  created_at timestamptz not null default now()
);

create index if not exists chat_rows_chat_seq on chat_rows (chat_id, seq);

-- ── istemcinin ürettiği anlık görüntü ───────────────────────────────────────
--
-- Kayıt büyüdüğünde istemci tüm doc'u dışa aktarıp buraya yazıyor; altındaki
-- satırlar artık gerekmiyor (`seq_floor`). Sunucu bunu kendisi üretmiyor.

create table if not exists chat_checkpoints (
  chat_id    text        primary key,
  owner      uuid        not null default auth.uid(),
  -- Anlık görüntünün kapsadığı en yüksek satır.
  seq        bigint      not null,
  payload    bytea       not null,
  updated_at timestamptz not null default now()
);

-- ── cihazlar ────────────────────────────────────────────────────────────────

create table if not exists devices (
  id         text        primary key,
  owner      uuid        not null default auth.uid(),
  name       text        not null,
  platform   text,
  last_seen  timestamptz not null default now()
);

-- ── erişim ──────────────────────────────────────────────────────────────────
--
-- Her satır sahibine bağlı ve yalnızca sahibi görüyor. Bu, çok kullanıcılı bir
-- kurulumda tek koruma katmanı: PostgREST doğrudan istemciden çağrıldığı için
-- yetkilendirme veritabanında olmak ZORUNDA, uygulama katmanında değil.

alter table chat_rows        enable row level security;
alter table chat_checkpoints enable row level security;
alter table devices          enable row level security;

do $$ begin
  create policy chat_rows_owner on chat_rows
    for all using (owner = auth.uid()) with check (owner = auth.uid());
exception when duplicate_object then null; end $$;

do $$ begin
  create policy chat_checkpoints_owner on chat_checkpoints
    for all using (owner = auth.uid()) with check (owner = auth.uid());
exception when duplicate_object then null; end $$;

do $$ begin
  create policy devices_owner on devices
    for all using (owner = auth.uid()) with check (owner = auth.uid());
exception when duplicate_object then null; end $$;

-- ── durum sorgusu ───────────────────────────────────────────────────────────
--
-- Protokolün STATE çerçevesi tek çağrıda üretiliyor: üç ayrı sorgu atmak
-- katılımı üç gidiş-dönüş yapardı.

create or replace function chat_state(p_chat_id text)
returns table (
  head_seq        bigint,
  seq_floor       bigint,
  checkpoint_seq  bigint,
  checkpoint_size bigint,
  row_count       bigint,
  row_bytes       bigint
)
language sql
stable
security invoker
as $$
  select
    coalesce((select max(seq) from chat_rows where chat_id = p_chat_id), 0),
    coalesce((select seq from chat_checkpoints where chat_id = p_chat_id), 0),
    coalesce((select seq from chat_checkpoints where chat_id = p_chat_id), 0),
    coalesce((select octet_length(payload) from chat_checkpoints where chat_id = p_chat_id), 0),
    (select count(*) from chat_rows where chat_id = p_chat_id),
    coalesce((select sum(octet_length(payload)) from chat_rows where chat_id = p_chat_id), 0);
$$;

-- ── sıkıştırma sonrası temizlik ─────────────────────────────────────────────
--
-- İstemci yeni bir anlık görüntü yazdıktan sonra çağırıyor. Ayrı bir fonksiyon
-- çünkü silme, anlık görüntünün YAZILDIĞI işlemden sonra olmalı: sıra tersine
-- dönerse kayıt silinir ama görüntü yazılmazsa geçmiş kaybolur.

create or replace function chat_trim(p_chat_id text, p_below bigint)
returns bigint
language sql
volatile
security invoker
as $$
  with gone as (
    delete from chat_rows
     where chat_id = p_chat_id
       and seq <= p_below
       and exists (
         -- Anlık görüntü gerçekten bu noktayı kapsıyor mu: kapsamıyorsa
         -- hiçbir şey silinmiyor.
         select 1 from chat_checkpoints c
          where c.chat_id = p_chat_id and c.seq >= p_below
       )
    returning 1
  )
  select count(*) from gone;
$$;

-- ── PostgREST arayüzü ───────────────────────────────────────────────────────
--
-- Okuma ve yazma tabloya doğrudan değil, fonksiyonlar üzerinden yapılıyor.
-- İki sebep:
--
-- 1. `payload` bir `bytea` ve JSON ikili veri taşıyamıyor. Fonksiyonlar
--    base64 `text` alıp veriyor; kodlama böylece sözleşmenin görünür parçası
--    oluyor, PostgREST'in `\x…` biçimine bağlı kalmıyoruz.
-- 2. Tekilleştirme ATOMİK olmak zorunda. İstemci bağlantı koptuğunda aynı
--    gönderimi tekrarlıyor; "önce var mı diye bak, sonra yaz" iki cihaz aynı
--    anda denediğinde ikisini de yazardı.

-- Aynı gönderim iki kez yazılamaz.
create unique index if not exists chat_rows_batch on chat_rows (chat_id, batch_id);

create or replace function chat_append(
  p_chat_id  text,
  p_device   text,
  p_batch_id text,
  p_payload  text
)
returns table (seq bigint, dup boolean)
language plpgsql
volatile
security invoker
as $$
declare
  v_seq bigint;
begin
  insert into chat_rows (chat_id, device, batch_id, payload)
  values (p_chat_id, p_device, p_batch_id, decode(p_payload, 'base64'))
  on conflict (chat_id, batch_id) do nothing
  returning chat_rows.seq into v_seq;

  if v_seq is not null then
    return query select v_seq, false;
    return;
  end if;

  -- Çakışma: satır zaten vardı. İstemcinin beklediği şey aynı sırayı geri
  -- almak — yeni bir sıra vermek onu ileri kaydırıp aradaki satırları
  -- atlatırdı.
  select r.seq into v_seq
    from chat_rows r
   where r.chat_id = p_chat_id and r.batch_id = p_batch_id;

  return query select v_seq, true;
end;
$$;

create or replace function chat_rows_after(
  p_chat_id text,
  p_after   bigint,
  -- Boş bırakılırsa süzme yok. İstemci kendi satırlarını geri almak
  -- istemediğinde kendi cihaz kimliğini veriyor.
  p_exclude_device text default null
)
returns table (seq bigint, device text, batch_id text, payload text)
language sql
stable
security invoker
as $$
  select r.seq, r.device, r.batch_id, encode(r.payload, 'base64')
    from chat_rows r
   where r.chat_id = p_chat_id
     and r.seq > p_after
     and (p_exclude_device is null or r.device <> p_exclude_device)
   order by r.seq;
$$;

-- Anlık görüntü okuma/yazma; aynı base64 gerekçesi.
create or replace function chat_checkpoint_put(
  p_chat_id text,
  p_seq     bigint,
  p_payload text
)
returns void
language sql
volatile
security invoker
as $$
  insert into chat_checkpoints (chat_id, seq, payload, updated_at)
  values (p_chat_id, p_seq, decode(p_payload, 'base64'), now())
  on conflict (chat_id) do update
    set seq = excluded.seq,
        payload = excluded.payload,
        updated_at = now()
    -- Eski bir anlık görüntü yenisini ezmemeli: iki cihaz aynı anda
    -- sıkıştırırsa geride kalan yazı kaybettirirdi.
    where excluded.seq >= chat_checkpoints.seq;
$$;

create or replace function chat_checkpoint_get(p_chat_id text)
returns table (seq bigint, payload text)
language sql
stable
security invoker
as $$
  select c.seq, encode(c.payload, 'base64')
    from chat_checkpoints c
   where c.chat_id = p_chat_id;
$$;

-- ── izinler ─────────────────────────────────────────────────────────────────
--
-- Proje kurulurken "Automatically expose new tables" KAPALI olmalı: açık
-- bırakmak her yeni tabloyu otomatik yayınlıyor ve bir gün eklenen bir tablo
-- kimse fark etmeden dışarı açılabiliyor. Kapalıyken erişim buradan, açıkça
-- veriliyor.
--
-- Yalnızca `authenticated` yetkilendiriliyor, `anon` DEĞİL. Bu bilinçli:
-- oturum açmamış bir çağrı tabloya hiç ulaşamıyor, RLS'e bile gelmeden
-- reddediliyor. Böylece `anon` anahtarının tek başına hiçbir şey açmadığı
-- iki katmanla garanti altında.

grant usage on schema public to authenticated;

grant select, insert, update, delete on chat_rows        to authenticated;
grant select, insert, update, delete on chat_checkpoints to authenticated;
grant select, insert, update, delete on devices          to authenticated;

-- `chat_rows.seq` kimlik sütunu: ekleme sırası diziyi de kullanıyor.
grant usage, select on all sequences in schema public to authenticated;

-- Fonksiyonlar `security invoker`, yani çağıranın haklarıyla çalışıyor ve
-- RLS aynen uygulanıyor. Çalıştırma hakkı yine de açıkça verilmeli.
grant execute on function chat_state(text)                        to authenticated;
grant execute on function chat_trim(text, bigint)                 to authenticated;
grant execute on function chat_append(text, text, text, text)     to authenticated;
grant execute on function chat_rows_after(text, bigint, text)     to authenticated;
grant execute on function chat_checkpoint_put(text, bigint, text) to authenticated;
grant execute on function chat_checkpoint_get(text)               to authenticated;
