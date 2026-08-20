# Supabase kurulumu

Postillion'ın bulut eşitlemesini kendi Supabase projenizde çalıştırmak için.

## 0. Proje kurulum seçenekleri

| seçenek | olmalı | neden |
|---|---|---|
| Enable Data API | **açık** | PostgREST bizim taşımamız; kapalıysa hiçbir şey çalışmaz |
| Automatically expose new tables | **kapalı** | Supabase'in kendi önerisi. Açık bırakmak ileride eklenen her tabloyu kimse fark etmeden yayınlar. `schema.sql` erişimi açıkça veriyor |
| Enable automatic RLS | **açık** | Şema RLS'i zaten kendi tabloları için açıyor; bu, ileride eklenecek tabloların da korumasız kalmamasını garantiliyor |

Bölge olarak size en yakınını seçin — her mesaj bir gidiş dönüş.

## 1. Şemayı uygulayın

Projenizin **SQL Editor**'ünde `schema.sql` dosyasının tamamını çalıştırın.
Tabloları, satır düzeyi güvenlik politikalarını ve PostgREST'in çağırdığı
fonksiyonları kuruyor. Yeniden çalıştırmak güvenli — hepsi `if not exists` /
`create or replace`.

## 2. Anahtarlar

Panelde **Project Settings → API** altında:

| değer | nerede kullanılıyor | gizli mi |
|---|---|---|
| Project URL | uygulama ayarı | hayır |
| `anon` anahtarı | uygulama ayarı | **hayır** — RLS'e tabi |
| `service_role` anahtarı | hiçbir yerde | **evet** — RLS'i atlar, asla dağıtmayın |

`anon` anahtarı istemci uygulamalarda açıkta durmak üzere tasarlanmış. Verinizi
koruyan şey anahtar değil, `schema.sql` içindeki politikalar: her satır
`owner = auth.uid()` ile bağlı ve kimse başkasının adına satır yazamıyor.

Yine de anahtarı uygulamaya **gömmeyin**, ayar olarak bırakın. Sebep güvenlik
değil kota: gömülü bir anahtar, uygulamayı kuran herkesin geçmişini sizin
projenize yazar ve ücretsiz planın 500 MB'ını sizin adınıza doldurur.

## 3. Doğrulama

Şema doğru kurulduysa ve RLS gerçekten koruyorsa bu iki test geçer:

```sh
export POSTILLION_SUPABASE_URL="https://<proje>.supabase.co"
export POSTILLION_SUPABASE_ANON_KEY="<anon anahtarı>"
export POSTILLION_SUPABASE_TOKEN="<kullanıcı erişim jetonu>"

cargo test -p postillion-sync --all-features --test supabase_live -- --ignored --nocapture
```

İkinci test kasıtlı olarak jetonsuz bağlanıp yazmayı DENİYOR ve başarısız
olmasını bekliyor. Geçmezse politikalar açık kalmış demektir — o durumda
anahtarı dağıtmak veriyi herkese açar.

### Erişim jetonu nereden

Kısa ömürlü olduğu için parola paylaşmaktan çok daha güvenli:

```sh
curl -s "$POSTILLION_SUPABASE_URL/auth/v1/token?grant_type=password" \
  -H "apikey: $POSTILLION_SUPABASE_ANON_KEY" \
  -H "Content-Type: application/json" \
  -d '{"email":"...","password":"..."}' | jq -r .access_token
```

## 4. Ücretsiz plan notu

Proje **7 gün** veritabanı hareketi olmazsa duruyor, uyanması ~30 saniye. Bir
eşitleme arka ucu için can sıkıcı: iki hafta kullanılmayan bir cihaz açıldığında
ilk deneme başarısız olabilir. Haftada bir basit bir sorgu atan zamanlanmış bir
iş bunu önlüyor.
