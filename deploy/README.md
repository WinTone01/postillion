# Kendi sunucunuza kurulum

Bu dizin `postillion-server`'ı bir VPS'e kurmak için gereken parçaları
içeriyor. Sunucunun tek işi sohbet satırlarını saklamak ve bağlı cihazlara
duyurmak — Cloudflare Durable Object'in yaptığı iş.

**Satır içerikleri sunucu için opak.** Protokol mantığı satırları hiç
açmıyor, oldukları gibi saklayıp iletiyor. Uçtan uca şifreleme henüz
DEVREDE DEĞİL ama mimari buna hazır: şifreleme eklendiğinde sunucu tarafında
değişmesi gereken bir şey yok.

## 1. Postgres

```sh
sudo -u postgres createuser postillion --pwprompt
sudo -u postgres createdb postillion --owner=postillion
```

Şema sunucu ilk açılışta kendini kuruyor; ayrı bir göç adımı yok.

## 2. İkiliyi derleyin ve kopyalayın

Sunucuda derlemek yerine kendi makinenizde derleyip kopyalamak daha hızlı
(VPS'lerin çoğunda Rust derlemesi için yeterli bellek yok):

```sh
cargo build --release -p postillion-server
scp target/release/postillion-server sunucu:/tmp/
```

Sunucuda:

```sh
sudo install -m 755 /tmp/postillion-server /usr/local/bin/
sudo useradd --system --no-create-home postillion
```

## 3. Yapılandırma

```sh
sudo mkdir -p /etc/postillion
sudo tee /etc/postillion/server.env >/dev/null <<'ENV'
DATABASE_URL=postgres://postillion:PAROLA@127.0.0.1/postillion
POSTILLION_SERVER_TOKEN=BURAYA_UZUN_RASTGELE_BIR_DEGER
ENV
sudo chmod 600 /etc/postillion/server.env
```

Jetonu üretmek için:

```sh
openssl rand -base64 48
```

Bu jeton şu an **tek kullanıcılık**: onu bilen herkes bütün sohbetlere
erişiyor. Kimseyle paylaşmayın; kayıt açılmadan önce (Aşama 3) yerini
gerçek oturumlar alacak.

## 4. Servis ve ters vekil

```sh
sudo cp deploy/postillion-server.service /etc/systemd/system/
sudo systemctl daemon-reload
sudo systemctl enable --now postillion-server
```

Sunucu yalnızca `127.0.0.1:8787` dinliyor; TLS'i nginx sonlandırıyor:

```sh
sudo cp deploy/nginx.conf /etc/nginx/sites-available/postillion
sudo ln -s /etc/nginx/sites-available/postillion /etc/nginx/sites-enabled/
sudo certbot --nginx -d sunucu.alanadiniz.com
```

`nginx.conf`'taki `Upgrade`/`Connection` başlıkları şart: onlarsız WebSocket
kurulmuyor ve eşitleme hiç başlamıyor.

## 5. Doğrulama

```sh
curl https://sunucu.alanadiniz.com/health
```

`ok` dönmeli.

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
  kota yok.
- **Uçtan uca şifreleme yok.** Sunucuya erişen biri satırların içeriğini
  okuyabilir. Kayıt açılmadan ÖNCE eklenmesi gerekiyor.

## Testler

Postgres testleri veritabanı olmadan atlanıyor:

```sh
docker run -d --rm --name pg -e POSTGRES_PASSWORD=test \
  -e POSTGRES_DB=postillion -p 55432:5432 postgres:17-alpine

POSTILLION_TEST_DATABASE_URL=postgres://postgres:test@127.0.0.1:55432/postillion \
  cargo test -p postillion-server
```
