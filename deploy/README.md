# Kendi sunucunuza kurulum

Bu dizin `postillion-server`'ı kendi sunucunuza kurmak için gereken parçaları
içeriyor. Sunucunun tek işi sohbet satırlarını saklamak ve bağlı cihazlara
duyurmak — Cloudflare Durable Object'in yaptığı iş.

**Satır içerikleri sunucu için opak.** Protokol mantığı satırları hiç
açmıyor, oldukları gibi saklayıp iletiyor. Uçtan uca şifreleme henüz
DEVREDE DEĞİL ama mimari buna hazır: şifreleme eklendiğinde sunucu tarafında
değişmesi gereken bir şey yok.

İki yol var: **Coolify** (önerilen — VPS'inizde zaten kurulu) ya da
**systemd** ile doğrudan kurulum.

---

## A. Coolify ile

### 1. Kaynağı oluşturun

Coolify panelinde: **Yeni Kaynak → Docker Compose**, bu depoyu seçin ve
compose dosyası olarak şunu verin:

```
deploy/docker-compose.yaml
```

Derleme bağlamı depo kökü (`context: ..`), çünkü sunucu birkaç crate'i
birden derliyor.

### 2. Değişkenler

`docker-compose.yaml` içindeki `SERVICE_PASSWORD_*` değişkenlerini Coolify
kendisi üretiyor:

| Değişken | İş |
| --- | --- |
| `SERVICE_PASSWORD_POSTGRES` | Veritabanı parolası |
| `SERVICE_PASSWORD_POSTILLIONTOKEN` | Cihazlarınızın kullanacağı jeton |
| `SERVICE_FQDN_SERVER_8787` | Coolify'ın atadığı alan adı |

Değişkenlerin tam listesi ve açıklamaları için [`.env.example`](.env.example).

Dağıtımdan sonra **Environment Variables** sekmesinden
`SERVICE_PASSWORD_POSTILLIONTOKEN` değerini okuyup istemcilere girin. Bu
değeri kimseyle paylaşmayın.

Coolify sürümünüz bu değişkenleri kendiliğinden üretmezse panelden elle
girin; üretmek için:

```bash
openssl rand -hex 32
```

### 3. Dağıtın ve doğrulayın

```bash
curl https://ATANAN-ALAN-ADI/health
```

`ok` dönmeli.

TLS'i, alan adını ve WebSocket yükseltmesini Coolify'ın vekili yönetiyor —
bu yüzden compose dosyasında nginx yok. Veritabanı `ports` yayınlamıyor,
yalnızca compose ağından erişilebiliyor.

### Derleme neden hızlı

Çalışma alanı imaj içinde kırpılıyor. `apps/postillion` ve `crates/ui`
gpui'ye bağlı, gpui de bir git bağımlılığı (zed çatalı, ~460 MB). Cargo
derlemeye başlamadan önce çalışma alanının tamamını çözdüğü için, kırpmasak
sunucunun hiç kullanmadığı yarım gigabaytlık bir depo her derlemede
inecekti. Kırpma yalnızca üye listesinden geçiyor; sürümler kök
`Cargo.toml`'da tek yerde kalıyor.

---

## B. systemd ile (Coolify olmadan)

### 1. Postgres

```bash
sudo -u postgres createuser postillion --pwprompt
```

```bash
sudo -u postgres createdb postillion --owner=postillion
```

Şema sunucu ilk açılışta kendini kuruyor; ayrı bir göç adımı yok.

### 2. İkiliyi derleyin ve kopyalayın

Sunucuda derlemek yerine kendi makinenizde derleyip kopyalamak daha hızlı:

```bash
cargo build --release -p postillion-server
```

```bash
scp target/release/postillion-server sunucu:/tmp/
```

Sunucuda:

```bash
sudo install -m 755 /tmp/postillion-server /usr/local/bin/
```

```bash
sudo useradd --system --no-create-home postillion
```

### 3. Yapılandırma

```bash
sudo mkdir -p /etc/postillion
```

Ardından `/etc/postillion/server.env` dosyasını oluşturun — alanlar
[`.env.example`](.env.example) içinde açıklanıyor:

```
DATABASE_URL=postgres://postillion:PAROLA@127.0.0.1/postillion
POSTILLION_SERVER_TOKEN=BURAYA_UZUN_RASTGELE_BIR_DEGER
```

```bash
sudo chmod 600 /etc/postillion/server.env
```

Jeton üretmek için:

```bash
openssl rand -hex 32
```

### 4. Servis ve ters vekil

```bash
sudo cp deploy/postillion-server.service /etc/systemd/system/
```

```bash
sudo systemctl daemon-reload
```

```bash
sudo systemctl enable --now postillion-server
```

Sunucu yalnızca `127.0.0.1:8787` dinliyor; TLS'i nginx sonlandırıyor:

```bash
sudo cp deploy/nginx.conf /etc/nginx/sites-available/postillion
```

```bash
sudo certbot --nginx -d sunucu.alanadiniz.com
```

`nginx.conf`'taki `Upgrade`/`Connection` başlıkları şart: onlarsız WebSocket
kurulmuyor ve eşitleme hiç başlamıyor.

---

## Uçlar

| Uç | İş |
| --- | --- |
| `GET /health` | Ayakta mı |
| `GET /chat2/{id}/ws` | Sohbet soketi (`?token=`) |
| `GET /chat2/{id}/rows` | WebSocket geçmeyen ağlar için satır çekme |
| `POST /chat2/{id}/rows` | Aynı yoldan gönderme |
| `GET /chat2/{id}/checkpoint` | Anlık görüntü — şu an her zaman 404 |

## Bilinen sınırlar

- **Anlık görüntü yok.** Odalar hiç budanmıyor: her yeni cihaz sohbetin tüm
  geçmişini indiriyor. Uzun sohbetlerde katılma süresi büyüyor.
- **Tek kullanıcı.** Yetkilendirme tek paylaşılan jeton; kullanıcı ayrımı ve
  kota yok. Jetonu bilen herkes bütün sohbetlere erişiyor.
- **Uçtan uca şifreleme yok.** Sunucuya erişen biri satırların içeriğini
  okuyabilir. Kayıt açılmadan ÖNCE eklenmesi gerekiyor.

## Yedekleme

Bütün durum Postgres'te. Coolify kurulumunda:

```bash
docker exec -t $(docker ps -qf name=db) pg_dump -U postillion postillion | gzip > yedek.sql.gz
```

## Testler

Postgres testleri veritabanı olmadan atlanıyor:

```bash
docker run -d --rm --name pg -e POSTGRES_PASSWORD=test -e POSTGRES_DB=postillion -p 55432:5432 postgres:17-alpine
```

```bash
POSTILLION_TEST_DATABASE_URL=postgres://postgres:test@127.0.0.1:55432/postillion cargo test -p postillion-server
```
