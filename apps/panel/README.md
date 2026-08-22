# Postillion paneli

Sohbetlerinize webden devam etmek için yönetim paneli. AdonisJS 6.

Tasarım kararları ve yol haritası: [`docs/panel-plan.md`](../../docs/panel-plan.md).

## Durum

Aşama 1 — iskelet ve kimlik. Kayıt, giriş, çıkış çalışıyor.

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
