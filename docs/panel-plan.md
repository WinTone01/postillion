# Uzaktan yönetim paneli — plan

Durum: **hiç yapılmadı.** Depoda AdonisJS ya da better-auth adına tek satır yok.
Bu belge ne yapılacağını ve neden öyle yapılacağını yazıyor.

## Panelin işi

Sırayla, en çok istenenden:

1. **Tarayıcıdan kendi cihazlarına mesaj göndermek.** Dizüstü kapalıyken bile
   VPS'teki ajana iş verebilmek.
2. Cihazları ve sohbetleri görmek.
3. Hesap yönetimi: kayıt, giriş, cihaz jetonları.

## Alınan kararlar

### Kimlik: `@adonisjs/auth`, better-auth değil

better-auth'un **AdonisJS entegrasyonu yok** (desteklenenler: Electron, Elysia,
Encore, Expo, Express, Fastify, Hono, Lynx, Next.js, Nuxt, SvelteKit,
SolidStart, Astro, düz Node). Çekirdeği Web standardı `Request`/`Response`
üzerinden çalıştığı için köprü yazılabilirdi, ama o köprünün bakımı bize
kalırdı — hem de en az ev yapımı kod isteyeceğimiz katmanda.

`@adonisjs/auth` ile her şey birinci taraf kalıyor ve Shield ile captcha-guard
doğal çalışıyor. Passkey/2FA sonradan gerekirse ayrıca eklenir.

### Güvenlik paketleri

| Paket | İş |
| --- | --- |
| `@adonisjs/shield` | CSRF, CSP, güvenlik başlıkları |
| `adonis-captcha-guard` | Kayıt ve giriş formlarında bot koruması |

Captcha için **Cloudflare Turnstile**: alan adı zaten Cloudflare'in arkasında,
ikinci bir sağlayıcı hesabı açmaya gerek yok.

### Panel Rust sunucusunun İKİNCİ istemcisi, kopyası değil

Eşitleme protokolünü TypeScript'te yeniden yazmıyoruz. Zaten üç uygulaması var
(Rust istemci, TS edge, Swift iOS) ve dördüncüsü elle senkron tutulacak dördüncü
bir sapma kaynağı olurdu.

Panel iki yoldan konuşuyor:

- **Okuma:** kayıt satırları düz JSON (`registry_rows.fields`). Kenar çubuğu —
  cihazlar, sohbetler, alanlar — CRDT'ye hiç dokunmadan okunabiliyor.
- **Yazma/eylem:** cihaz rölesine `role=client` ile bağlanıp mevcut RPC'yi
  çağırıyor. Uzaktan mesaj göndermenin tasarlanmış yolu bu.

Sohbet İÇERİĞİ opak loro güncellemesi; onu göstermek tarayıcıda loro gerektiriyor.
`loro-crdt` npm'de var ve `edge/src/session-doc/` okuma mantığını zaten taşıyor,
yani mümkün — ama ilk sürümün kapsamı dışında.

### Jeton modeli: sunucu kendi tablosunu tutuyor

Panel Adonis'in oturumlarını kendi içinde kullanıyor. Rust sunucusu ise **kendi**
`api_tokens` tablosunu doğruluyor: panel bir cihaz jetonu üretince oraya satır
yazıyor, sunucu sunulan jetonu kendi kuralıyla özetleyip arıyor.

Alternatif Adonis'in `auth_access_tokens` tablosunu doğrudan okumaktı; o,
sunucuyu Adonis'in iç özetleme biçimine bağlardı ve çerçevenin bir sürümü onu
değiştirdiğinde eşitleme sessizce kırılırdı.

Bu aynı zamanda bugünkü **tek paylaşılan jetonu** emekliye ayırıyor — planın
Aşama 3'ü.

## Aşamalar

### 1. İskelet ve kimlik
- AdonisJS 6 projesi (`apps/panel`), aynı Postgres
- `@adonisjs/auth` oturum guard'ı, kayıt + giriş
- Shield ve captcha-guard kurulumu
- Uygulamanın renk teması (`crates/ui/src/theme.rs` → CSS değişkenleri)

### 2. Sunucu tarafı kimlik
- `api_tokens` tablosu ve sunucuda doğrulama
- Panelde jeton üretme/iptal arayüzü
- Tek paylaşılan jeton kaldırılıyor
- Kullanıcı ayrımı: her sorgu kullanıcıyla sınırlı

### 3. Cihazlar ve sohbetler
- Kayıt satırlarından cihaz ve sohbet listesi
- Presence: hangi cihaz çevrimiçi

### 4. Mesaj gönderme
- Cihaz rölesine `role=client` bağlantısı
- Hedef cihaz + sohbet seçip mesaj gönderme
- Cevabın akışını izleme

### 5. Transkript (isteğe bağlı)
- `loro-crdt` + `edge/src/session-doc/` ile sohbet içeriği

## Açık riskler

**Uçtan uca şifreleme paneli kısıtlıyor.** Planda E2EE kayıt açılmadan önce
geliyor. Şifreleme devreye girdiğinde sunucu sohbet içeriğini okuyamaz — panel
de okuyamaz. Tarayıcıda çözmek anahtarı oraya koymak demek ve E2EE'nin
söylediğini zayıflatır. Karar Aşama 5'ten önce verilmeli: panel içeriği görsün
mü, yoksa yalnızca gönderme ve durum paneli mi olsun.

**Röle üzerinden terminal akışı çalışmıyor** (issue #5). Panelde uzaktan
terminal düşünülüyorsa önce o kapatılmalı.

**Kayıt açmak kota gerektirir.** Bugün sunucuda kullanıcı başına sınır yok;
herkese açık kayıt bunu zorunlu kılar.
