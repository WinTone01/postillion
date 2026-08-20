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
