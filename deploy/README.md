# Kendi sunucunuza kurulum

Bu dizin `postillion-server`'ı kendi sunucunuza kurmak için gereken parçaları
içeriyor. Sunucunun tek işi sohbet satırlarını saklamak ve bağlı cihazlara
duyurmak — Cloudflare Durable Object'in yaptığı iş.

**Sunucu satırları hiç açmıyor**, oldukları gibi saklayıp iletiyor. Ama bu
gizlilik değil sadeleştirme: içerik şifreli değil, yalnızca sunucunun
ayrıştırmasına gerek yok. Veritabanına erişen okuyabilir.

Uçtan uca şifreleme **eklenmeyecek** — eksiklik değil, alınmış bir karar
(gerekçesi: `docs/panel-plan.md`). Web panelinin işi sohbete tarayıcıdan devam
etmek ve içeriği şifrelemek, göstermek için anahtarı tarayıcıya koymayı
gerektirirdi.

İki yol var: **Coolify** (önerilen — VPS'inizde zaten kurulu) ya da
**systemd** ile doğrudan kurulum.

---

## A. Coolify ile

### 1. Kaynağı oluşturun

Coolify panelinde: **Yeni Kaynak → Docker Compose**, bu depoyu seçin ve
compose dosyası olarak şunu verin:

```
docker-compose.yaml
```

Compose dosyaları depo **kökünde** duruyor. Coolify proje dizinini depo kökü
yapıyor ve derleme bağlamını compose dosyasının bulunduğu yere değil ona göre
çözüyor; alt dizinde duran bir compose'un göreli yolları bu yüzden
tutmuyordu.

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

## C. Panel (tanıtım sayfası dahil)

Panel ve tanıtım sayfası **tek kaynak**: aynı alan adında iki dağıtım tutmak,
uygulamanın statik dosya sunucusunun zaten yaptığı işi ikinci kez kurmak
olurdu.

| Yol | Ne |
| --- | --- |
| `/` | Tanıtım sayfası (statik) |
| `/login`, `/register` | Kimlik |
| `/app` | Panel — cihazlar, sohbetler, jetonlar |

Coolify'da: **Yeni Kaynak → Docker Compose**, aynı depoyu seçin ve compose
dosyası olarak şunu verin:

```
docker-compose.panel.yaml
```

**Domains** alanına ana alan adınızı yazın (örn. `postillion.net`).

Veritabanı eşitleme sunucusuyla **AYNI** olmalı — panel kayıt satırlarını ve
jeton tablosunu oradan okuyor. `PANEL_DB_*` değişkenlerini ona göre girin.

| Alan adı | Kaynak |
| --- | --- |
| `postillion.net` | `docker-compose.panel.yaml` |
| `sync.postillion.net` | `docker-compose.yaml` |

Depoda ayrıca Cloudflare Workers'a dağıtan bir iş akışı duruyor
(`.github/workflows/deploy.yml`). `CLOUDFLARE_API_TOKEN` sırrı tanımlı
değilken hiçbir şey yapmıyor, dolayısıyla Coolify kurulumuyla çakışmıyor.

---

## Uçlar

| Uç | İş |
| --- | --- |
| `GET /` | Sunucuyu tanıtır |
| `GET /health` | Ayakta mı |
| `GET /chat2/{id}/ws` | Sohbet soketi (`?token=`) |
| `GET /chat2/{id}/rows` | WebSocket geçmeyen ağlar için satır çekme |
| `POST /chat2/{id}/rows` | Aynı yoldan gönderme |
| `GET /chat2/{id}/checkpoint` | Anlık görüntü — şu an her zaman 404 |
| `GET /chat2/{id}/messages` | Materyalize transkript (panel için; eşitleme istemcileri kullanmıyor) |
| `GET /registry/{org}/ws` | Çalışma alanı kaydı: kenar çubuğu satırları, presence |
| `GET /registry/{org}/rows` | Kaydın HTTP yolu |
| `POST /registry/{org}/push` | Aynı yoldan yazma |
| `GET /device/{id}/ws` | Cihazlar arası RPC rölesi (`?role=host\|client`) |

Uygulama bu uçların **hepsine** ihtiyaç duyuyor. Yalnızca `chat2` sunan bir
sürüm, kayıt ucundan `404` aldığı için sonsuza kadar "Reconnecting" kalıyordu —
`/health` `200` dönerken.

## Cloudflare arkasında çalıştırıyorsanız

Alan adınız Cloudflare vekilinden (turuncu bulut) geçiyorsa **Browser
Integrity Check / Bot Fight Mode eşitlemeyi tamamen kırıyor.** Bu kontroller
tarayıcı imzası arıyor; masaüstü istemcisi bir tarayıcı değil ve WebSocket
yükseltmesi `error code: 1010` ile reddediliyor. Tarayıcıdan `/health`
çağırdığınızda `ok` görmeniz yanıltıcı: tarayıcı kontrolü geçiyor, istemci
geçmiyor.

Üç seçenek var:

1. **WAF kuralı (önerilen).** Cloudflare panelinde Security → WAF → Custom
   rules: `Hostname eşittir sync.alanadiniz.com` için eylem **Skip** →
   Browser Integrity Check. Alan adının geri kalanı korumalı kalıyor.
2. **Alt alan adını gri buluta çekin.** DNS kaydını "DNS only" yapın; TLS'i
   Coolify sonlandırır. En basiti, ama sunucunun IP'si açığa çıkar ve
   Cloudflare'in DDoS koruması devreden çıkar.
3. Bot Fight Mode'u tüm bölge için kapatın — landing sayfasını da
   korumasız bırakır, o yüzden en az tercih edileni.

Doğru yapılandırdığınızı şöyle sınayabilirsiniz — tarayıcı User-Agent'ı
OLMADAN istemek önemli, kontrolü tetikleyen şey tam olarak bu:

```bash
curl -sS -o /dev/null -w '%{http_code}\n' -H 'User-Agent:' https://sync.alanadiniz.com/health
```

`200` dönmeli. `403` dönüyorsa kontrol hâlâ açık.

Asıl sınama WebSocket yükseltmesi. `--http1.1` ŞART: `curl` TLS üzerinde
varsayılan olarak HTTP/2 pazarlıyor ve klasik `Upgrade` el sıkışması HTTP/2'de
geçersiz, dolayısıyla istek sunucuya varmadan `400` ile reddediliyor — bu
sunucunun değil komutun hatası olur. Rust istemcisi HTTP/1.1 ile el sıkışıyor.

```bash
curl -sS --http1.1 -o /dev/null -w '%{http_code}\n' -H 'User-Agent:' -H 'Connection: Upgrade' -H 'Upgrade: websocket' -H 'Sec-WebSocket-Version: 13' -H 'Sec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==' "https://sync.alanadiniz.com/chat2/deneme/ws?token=JETON"
```

Cevabın anlamı:

| Kod | Ne demek |
| --- | --- |
| `101` | Her şey yolunda: Cloudflare geçildi, jeton kabul edildi, soket açıldı |
| `401` | Jeton yanlış — sunucuya ulaşıyorsunuz, kimlik denetimi reddediyor |
| `403` | Cloudflare engelliyor; WAF kuralı eksik ya da yanlış alan adında |
| `400` | Büyük ihtimalle `--http1.1` unutulmuş |

## Kimlik ve izolasyon

İki jeton kaynağı var:

| Kaynak | Kim |
| --- | --- |
| `POSTILLION_SERVER_TOKEN` | Tek kullanıcılık kip — bütün odalara erişir |
| Panelden üretilen jetonlar | Kendi kullanıcısının odalarına erişir |

Odalar **ilk yazan sahiplenir** kuralıyla korunuyor: sahipsiz bir odaya giren
ilk kimlik onu alıyor, sonrakiler reddediliyor (`403`). Oda kimlikleri
(`chatId`, `org`, cihaz kimliği) istemci tarafından üretildiği ve tahmin
edilebilir olduğu için tek koruma bu.

Jetonlar veritabanında yalnızca **SHA-256 özeti** olarak duruyor; ham jeton
hiçbir yerde saklanmıyor.

## Bilinen sınırlar

- **Anlık görüntü yok.** Odalar hiç budanmıyor: her yeni cihaz sohbetin tüm
  geçmişini indiriyor. Uzun sohbetlerde katılma süresi büyüyor.
- **Kota yok.** Kullanıcı başına sınır bulunmuyor; herkese açık kayıt bunu
  zorunlu kılar.
- **Paylaşılan jeton bir ANA ANAHTAR.** `POSTILLION_SERVER_TOKEN` bütün
  odalara erişiyor ve sahiplik denetimini atlıyor. Panelden jeton
  ürettikten sonra kaldırın; sunucu açılışta bunu uyarı olarak yazıyor.
- **Şifreleme yok, bilerek.** Sunucuyu işleten sohbetleri okuyabilir ve
  veritabanı yedeği onların düz kopyasıdır. Kendinize ait tek kullanıcılık bir
  sunucuda sorun değil; başkalarına kayıt açarsanız bunu onlara söylemeniz
  gerekir.

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
