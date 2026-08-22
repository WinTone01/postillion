# Uzaktan yönetim paneli — plan

Durum: **hiç yapılmadı.** Depoda AdonisJS ya da better-auth adına tek satır yok.
Bu belge ne yapılacağını ve neden öyle yapılacağını yazıyor.

## Panelin işi

**Bilgisayarı açık olan bir kullanıcının, o bilgisayardaki sohbete webden devam
edebilmesi.**

Tek cümle ama iki şey söylüyor: transkripti görebilmek ve içine yazabilmek. Ve
"bilgisayar açıksa" şartı bir sınır değil, tasarımın kendisi — panel işi
bilgisayara yaptırıyor, kendisi yapmıyor.

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

### Panel bir RPC istemcisi; eşitleme istemcisi DEĞİL

Bu planın en önemli kararı ve işi büyük ölçüde ortadan kaldırıyor.

İlk düşünce, sohbet içeriğini göstermek için tarayıcıda loro çalıştırmaktı:
satırlar opak CRDT güncellemesi ve okumak için birleştirme gerekiyor. Gerekmiyor.
Motorun RPC yüzeyinde iki metot **zaten** `targetDeviceId` ile uzak cihaza
yönlendirilebilir durumda (`crates/engine/src/rpc.rs`, `forwardable`):

| Metot | Panelde karşılığı |
| --- | --- |
| `WatchDocMessages` | Transkripti akış olarak al |
| `QueueCommand` | Sohbete mesaj yaz |

Yani panel cihaz rölesine `role=client` ile bağlanıp bu ikisini çağırıyor.
Belgeyi host tutuyor, mesajları host materyalize ediyor, panel yalnızca JSON
render ediyor. Tarayıcıda CRDT yok, protokolün dördüncü bir uygulaması yok.

Eşitleme protokolünün zaten üç uygulaması var (Rust istemci, TS edge, Swift iOS)
ve elle senkron tutuluyorlar; dördüncüsü dördüncü bir sapma kaynağı olurdu.

Kayıt satırları (`registry_rows.fields`) düz JSON olduğu için cihaz ve sohbet
LİSTESİ bilgisayar kapalıyken de gösterilebiliyor. Gösterilemeyen şey o
sohbetin içi — çünkü onu materyalize eden bilgisayar kapalı. İstenen model bu.

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

### 4. Sohbete devam etme — panelin asıl işi
- Cihaz rölesine `role=client` bağlantısı
- `WatchDocMessages` ile transkript
- `QueueCommand` ile mesaj yazma
- Cevabın akışını canlı izleme

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

**Bilgisayar kapalıyken sohbet açılamaz.** Tasarımın kendisi, ama arayüzde
bunun net görünmesi gerekiyor: kullanıcı boş bir ekranla değil "bu cihaz
çevrimdışı" ile karşılaşmalı.
