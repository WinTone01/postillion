# Postillion paneli

Sohbetlerinize webden devam etmek için yönetim paneli. AdonisJS 6.

Tasarım kararları ve yol haritası: [`docs/panel-plan.md`](../../docs/panel-plan.md).

## Durum

Aşama 1–3. Kayıt/giriş, cihaz jetonları ve çalışma alanı listesi (cihazlar
ve sohbetler, canlılık göstergesiyle) çalışıyor.

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

`.env.test` `SESSION_DRIVER=memory` kullanıyor ve bu şart: test istemcisi
oturuma sunucu tarafından yazıyor, cookie sürücüsünde bu mümkün değil ve her
form testi CSRF'e takılıyor.
