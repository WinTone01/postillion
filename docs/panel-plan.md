# Uzaktan yönetim paneli — plan

Durum: **hiç yapılmadı.** Depoda AdonisJS ya da better-auth adına tek satır yok.
Bu belge ne yapılacağını ve neden öyle yapılacağını yazıyor.

## Panelin işi

**Sohbetlere webden devam edebilmek.**

İki ayrı yetenek ve şartları farklı:

| Yetenek | Bilgisayar gerekiyor mu |
| --- | --- |
| Transkripti okumak | **Hayır** — sunucu kendi kuruyor |
| Sohbete yazmak | Evet — işi yapan ajan orada çalışıyor |

Okumanın bilgisayardan bağımsız olması şart: kapalı bir dizüstü yüzünden
geçmişin görünmemesi kabul edilemez. İleride bulut çalıştırma geldiğinde
yazmak da bağımsızlaşacak — o zaman ajanı sunucu çalıştıracak.

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

### Okuma sunucuda materyalize ediliyor — YAPILDI

Satırlar opak loro güncellemesi; okumak için birleştirme gerekiyor. Bunu üç
yerde yapmak mümkündü ve seçim önemliydi:

- **Host'a sordurmak.** Elenmesinin sebebi tam da bilgisayarın kapalı
  olabilmesi: soracak kimse yok.
- **Tarayıcıda loro.** Mümkün (`loro-crdt` npm'de) ama bulut çalıştırma
  geldiğinde aynı işi sunucuda ikinci kez yazmak gerekirdi.
- **Sunucuda.** Birleştirme kodu zaten burada (`postillion-doc`, istemcinin
  çalıştırdığının aynısı) ve bulut çalıştırmanın da ihtiyacı olan şey bu.

Uygulandı: `crates/server/src/transcript.rs` satırları birleştirip mesajları
okuyor, `GET /chat2/{id}/messages` panele JSON veriyor. Bozuk ya da bağımlılığı
eksik bir satır bütün transkripti düşürmüyor — elde olan gösteriliyor.

### Yazma cihaz rölesinden

Mesaj yazmak ajanı çalıştırmak demek ve ajan bilgisayarda. Motorun RPC yüzeyinde
`QueueCommand` **zaten** `targetDeviceId` ile uzak cihaza yönlendirilebilir
(`crates/engine/src/rpc.rs`, `forwardable`), yani panel cihaz rölesine
`role=client` ile bağlanıp onu çağırıyor.

Eşitleme protokolünü TypeScript'te yeniden yazmıyoruz: zaten üç uygulaması var
(Rust istemci, TS edge, Swift iOS) ve elle senkron tutuluyorlar.

Cihaz ve sohbet LİSTESİ de bilgisayardan bağımsız: kayıt satırları
(`registry_rows.fields`) düz JSON.

### Jeton modeli: sunucu kendi tablosunu tutuyor

Panel Adonis'in oturumlarını kendi içinde kullanıyor. Rust sunucusu ise **kendi**
`api_tokens` tablosunu doğruluyor: panel bir cihaz jetonu üretince oraya satır
yazıyor, sunucu sunulan jetonu kendi kuralıyla özetleyip arıyor.

Alternatif Adonis'in `auth_access_tokens` tablosunu doğrudan okumaktı; o,
sunucuyu Adonis'in iç özetleme biçimine bağlardı ve çerçevenin bir sürümü onu
değiştirdiğinde eşitleme sessizce kırılırdı.

Bu aynı zamanda bugünkü **tek paylaşılan jetonu** emekliye ayırıyor — planın
Aşama 3'ü.

## İlerideki yön: bulutta çalıştırma

Panelden yazmanın bilgisayara bağlı olması geçici. İleride ajan sunucuda da
çalışacak ve webde başlatılan iş cihazlara dağıtılacak. Bugünkü karar buna
göre alındı: transkripti sunucuda materyalize etmek o zaman zaten gerekecek —
sunucu sohbeti okuyamadan içine yazamaz.

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

### 4. Sohbete devam etme — panelin asıl işi
- Transkript `GET /chat2/{id}/messages` ile (sunucu tarafı hazır)
- Yazmak için cihaz rölesine `role=client` bağlantısı + `QueueCommand`
- Cihaz çevrimdışıysa: geçmiş okunur, yazma kapalı ve sebebi yazılı

### 5. Cilalama
- Yeniden bağlanma, host çevrimdışıyken açık durum bildirimi
- Mobil düzen

## Uçtan uca şifreleme YAPILMIYOR

Bilinçli karar. Sonuçları açıkça yazılı olmalı:

- Sunucuyu işleten sohbet içeriğini **okuyabilir**. Kendi sunucunuzda tek
  kullanıcıyken bu bir sorun değil; başkalarına kayıt açıldığında onların
  bunu bilmesi gerekir.
- Veritabanı yedeği sohbetlerin düz kopyasıdır; nereye konduğu önemlidir.

Buna karşılık panel mümkün oluyor: şifreli olsaydı host'un materyalize ettiği
transkripti panele taşımak anahtarı tarayıcıya koymayı gerektirirdi ve bu
şifrelemenin söylediğini zaten zayıflatırdı.

## Açık riskler

**Röle üzerinden terminal akışı çalışmıyor** (issue #5). Panelde uzaktan
terminal düşünülüyorsa önce o kapatılmalı. Sohbete devam etmek bundan
etkilenmiyor — o ayrı bir RPC yolu.

**Kayıt açmak kota gerektirir.** Bugün sunucuda kullanıcı başına sınır yok;
herkese açık kayıt bunu zorunlu kılar.

**Bilgisayar kapalıyken yazılamaz.** Okumak çalışıyor; yazmak ajanı
çalıştırmak demek ve ajan orada. Arayüz bunu net söylemeli — kullanıcı
kaybolmuş bir buton değil "bu cihaz çevrimdışı" görmeli. Bulut çalıştırma
geldiğinde bu kısıt kalkacak.
