# Postillion paneli

Sohbetlerinize webden devam etmek için yönetim paneli. AdonisJS 6.

Tasarım kararları ve yol haritası: [`docs/panel-plan.md`](../../docs/panel-plan.md).

## Yollar

Tanıtım sayfası ve panel **tek uygulama**, tek alan adı:

| Yol | Ne |
| --- | --- |
| `/` | Tanıtım sayfası — `public/index.html`, statik |
| `/login`, `/register` | Kimlik |
| `/verify` | E-posta doğrulama bekleme ekranı |
| `/app` | Panel |

Vite çıktısı `public/build` altında, `public/assets` DEĞİL: ikincisi tanıtım
sayfasının görsellerinin yeri ve Vite derlemede o dizini temizliyordu —
yerelde görünmeyen, yalnızca imajda ortaya çıkan bir kayıp.

## Durum

Aşama 1–5 tamamlandı. Kayıt/giriş, cihaz jetonları, çalışma alanı listesi,
sohbete devam etme ve canlı akış çalışıyor.

## E-posta doğrulama

Kayıttan sonra imzalı bir bağlantı gönderiliyor (Mailtrap ya da herhangi bir
SMTP). Sunucuda saklanan jeton **yok**: süre imzanın içinde (24 saat) ve
temizlenmeyi bekleyen bir tablo doğmuyor.

Doğrulanmamış hesap giriş yapabiliyor ama `/app` yerine `/verify` ekranına
iniyor. Engellenen şey panelin kendisi: oradan gerçek makinelere komut
gidiyor. Kendi durumunu görmesi ve postayı yeniden isteyebilmesi gerektiği
için giriş serbest.

`SMTP_HOST`/`SMTP_USERNAME` boşken doğrulama **şartı koşulmuyor** ve posta
gönderilmiyor. Şart koşulsaydı bağlantı hiç ulaşmayacağı için herkes kalıcı
olarak dışarıda kalırdı. `APP_URL` doldurulmalı: postadaki bağlantı göreli
olamaz.

### Mailtrap

Mailtrap'in iki ayrı ürünü var ve karıştırmak kolay:

| | Sandbox | Sending |
|---|---|---|
| Sunucu | `sandbox.smtp.mailtrap.io` | `live.smtp.mailtrap.io` |
| Port | 2525 | 587 |
| Kullanıcı | kutuya özel | `api` |
| Parola | kutuya özel | API jetonu |
| Postayı alan | yalnızca Mailtrap kutusu | gerçek adres |

Sandbox posta **teslim etmiyor**, kendi kutusunda tutuyor. Geliştirmede
doğru seçim; üretimde kullanıcı doğrulama bağlantısını hiç görmez.

Üretim kurulumu (Sending):

1. **Sending Domains → Add Domain** ile alan adını ekleyin
   (`postillion.net`), verilen DNS kayıtlarını (SPF, DKIM, DMARC) yayınlayın
   ve doğrulanmasını bekleyin. Doğrulanmamış alan adından gönderim
   reddediliyor.
2. **API Tokens** altından bir jeton üretin.
3. Coolify'da:

```
SMTP_HOST=live.smtp.mailtrap.io
SMTP_PORT=587
SMTP_USERNAME=api
SMTP_PASSWORD=<API jetonu>
MAIL_FROM_ADDRESS=noreply@postillion.net
MAIL_FROM_NAME=Postillion
APP_URL=https://postillion.net
```

`MAIL_FROM_ADDRESS` doğrulanmış alan adında olmalı — başka bir alan adı
yazmak gönderimi reddettirir.

`APP_URL` boş bırakılırsa posta **hiç gönderilmiyor**. Göreli bir bağlantı
postada işe yaramıyor: tarayıcı ilk parçayı sunucu adı sanıyor ve
`https://verify/1` açmaya çalışıyor. Coolify'da bu değer compose'daki
`SERVICE_FQDN_PANEL_3333` üzerinden geliyor; panelin alan adı orada
tanımlı değilse elle `APP_URL` yazın.

### Yeniden gönderim

Bekleme ekranındaki buton kullanıcı başına dakikada bir çalışıyor. Buton
bir saldırı aracı: saldırgan başkasının adresiyle kaydolup basmaya devam
ederek o kutuyu doldurabilirdi.

Süre oturumda değil `users.verification_sent_at` sütununda: oturumda
tutulsaydı çerezleri silmek onu sıfırlardı. Butonun devre dışı görünmesi
yalnızca nezaket, şartı uygulayan sunucu.

## Canlı akış

Sayfa açıkken transkript kendini yeniliyor. Tarayıcı eşitleme sunucusuna
**hiç gitmiyor** — panel vekillik ediyor. Gitseydi sunucu jetonunun tarayıcıya
verilmesi gerekirdi ve o jeton kullanıcının bütün odalarına açılıyor.

Yoklama ucuz: panel elindeki baş sırayı gönderiyor ve değişmemişse sunucu
belgeyi hiç kurmuyor. Aksi hâlde uzun bir sohbette hiç değişmemiş bir belge
saniyede bir yeniden birleştirilirdi.

Art arda hatada aralık açılıyor (3 sn → 30 sn) ve ekrana hata basılmıyor:
geçici bir arıza, okunabilir duran bir transkripti gürültüye boğmamalı.

## Sohbete devam etme

Okumak ve yazmak farklı yollardan geliyor ve şartları farklı:

| | Nereden | Cihaz gerekiyor mu |
| --- | --- | --- |
| Transkript | `GET /chat2/{id}/messages` | **Hayır** — satırları sunucu birleştiriyor |
| Mesaj yazmak | Cihaz rölesi → `QueueCommand` | Evet — ajan orada çalışıyor |

Cihaz kapalıyken geçmiş okunuyor, yazma kutusu görünmüyor ve **sebebi
yazılı**: kaybolmuş bir kutu soru bırakırdı.

Röle `host_offline` döndürdüğünde kullanıcı bunu anında görüyor — zaman aşımı
beklemek 15 saniye boşuna bekletirdi.

## Cihaz ve sohbet listesi

Listeler `registry_rows` tablosundan okunuyor: bu satırlar düz JSON,
dolayısıyla CRDT çalıştırmaya gerek yok ve **bilgisayar kapalıyken de**
görünüyorlar.

Canlılık farklı bir yerden geliyor. Kayıt satırlarındaki `lastSeenAt` bu
soruyu CEVAPLAMIYOR — o yalnızca açılış ve kapanışta yazılıyor, dolayısıyla
açık duran bir cihaz orada saatler öncesinde görünür. Canlı atışlar sunucunun
belleğinde ve `GET /registry/{org}/presence` ile okunuyor.

`POSTILLION_SERVER_URL` ayarlanmazsa panel yine çalışıyor: listeler
görünüyor, durum "bilinmiyor" yazıyor. "Çevrimdışı" demiyor — açık bir cihazı
kapalı göstermek yanlış bilgi olurdu.

## Cihaz jetonları

Kullanıcı `/tokens` sayfasından jeton üretiyor ve cihazın **Ayarlar →
Eşitleme** ekranına giriyor.

Ham jeton **yalnızca üretildiği anda, bir kez** gösteriliyor. Saklanan tek şey
SHA-256 özeti, dolayısıyla geri getirilemiyor — kaçırılırsa yenisini üretmek
gerekiyor. Bu bilinçli: veritabanını ele geçiren biri jetonları kullanamıyor.

Jeton **onaltılık**, base64 değil: istemci tarafında bu değer kullanıcı
kimliği olarak da kullanılıp veri dizininde bir yol parçasına dönüşüyor ve
base64'teki `/` o yolu bölerdi.

`api_tokens` tablosunun şeması BURADA tanımlı değil; `postillion-server`
açılışta kuruyor (`crates/server/src/identity_db.rs`). İki yerde tanımlamak,
ikisinin zamanla ayrılması ve hangisinin önce koştuğuna bağlı bir veritabanı
demekti. Panel çalışmadan önce sunucunun bir kez başlamış olması gerekiyor.

Özetleme iki tarafta AYNI olmak zorunda ve paylaşılan bir test vektörüyle
bağlanıyor (`ApiToken.hash` ↔ `hash_token`). Ayrılsalardı hiçbir jeton
doğrulanmaz ve hata yalnızca çalışan sistemde görünürdü.

## Kurulum

```bash
npm install
```

`.env` dosyasını `.env.example`'dan türetin. Veritabanı **sunucununkiyle
aynı** Postgres olmalı: panel ilerleyen aşamalarda kayıt satırlarını oradan
okuyacak.

```bash
node ace migration:run
```

```bash
npm run dev
```

## Güvenlik

| Katman | Ne yapıyor |
| --- | --- |
| `@adonisjs/shield` | CSRF, CSP, güvenlik başlıkları |
| `adonis-captcha-guard` | Kayıt ve giriş formlarında Cloudflare Turnstile |
| `@adonisjs/auth` | Oturum tabanlı kimlik, `scrypt` ile parola özeti |

**Turnstile anahtarları boşken captcha doğrulaması ATLANIYOR.** Geliştirme ve
testte hesap olmayabilir diye böyle; üretimde doldurmazsanız kayıt formu
botlara açık kalır.

Captcha girişte de var. Yalnızca kaydı korumak, parola deneme saldırısına
kapıyı açık bırakırdı.

## Tema

Renkler masaüstü uygulamasının temasından **üretiliyor**
(`resources/css/theme.css`). Elle düzenlemeyin — `crates/ui/src/theme.rs`
değiştiğinde şu test düşer ve nasıl güncelleneceğini söyler:

```bash
cargo test -p postillion-ui --test panel_theme
```

## Testler

Postgres gerekiyor:

```bash
docker run -d --rm --name panel-pg -e POSTGRES_PASSWORD=t -e POSTGRES_USER=panel -e POSTGRES_DB=panel -p 55433:5432 postgres:17-alpine
```

```bash
node ace test
```

Göçler test koşusunun başında otomatik uygulanıyor.

`.env.test` `SESSION_DRIVER=memory` kullanıyor ve bu şart: test istemcisi
oturuma sunucu tarafından yazıyor, cookie sürücüsünde bu mümkün değil ve her
form testi CSRF'e takılıyor.
