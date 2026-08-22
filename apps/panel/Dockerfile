# Panel imajı — tanıtım sayfasını da bu sunuyor.
#
# Ayrı bir landing dağıtımı vardı ve kaldırıldı: aynı alan adında iki kaynak
# tutmak, tek uygulamanın statik dosya sunucusunun zaten yaptığı işi ikinci
# kez kurmak demekti.

FROM node:22-slim AS builder

WORKDIR /app

# Bağımlılıklar ayrı katmanda: kaynak değiştiğinde yeniden kurulmasınlar.
COPY apps/panel/package.json apps/panel/package-lock.json ./
RUN npm ci

COPY apps/panel/ ./
RUN node ace build --ignore-ts-errors

# Üretim bağımlılıkları derlenen çıktının yanına.
WORKDIR /app/build
RUN npm ci --omit=dev

FROM node:22-slim AS runtime

# `dumb-init` yok: Node PID 1 olarak sinyalleri işliyor ve Adonis kapanışı
# kendi yönetiyor.
WORKDIR /app
ENV NODE_ENV=production

COPY --from=builder /app/build ./

# Kök olmayan kullanıcı: imajda `node` zaten var.
USER node

EXPOSE 3333
ENV HOST=0.0.0.0 PORT=3333

# Göçler açılışta koşuyor.
#
# Coolify compose'unda ayrı bir "release" adımı yok ve elle çalıştırmayı
# belgeye yazmak, ilk dağıtımda "users tablosu yok" hatasıyla karşılaşmak
# demekti. Adonis göç tablosunu kilitliyor, ama bu yine de TEK ÖRNEK
# varsayıyor: panel ölçeklenirse bu adım ayrılmalı.
CMD ["sh", "-c", "node ace migration:run --force && node bin/server.js"]
