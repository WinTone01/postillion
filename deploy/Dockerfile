# postillion-server imajı.
#
# Çalışma alanı KIRPILIYOR: `apps/postillion` ve `crates/ui` gpui'ye bağlı ve
# gpui bir git bağımlılığı (zed çatalı, ~460 MB). Cargo derlemeye başlamadan
# önce çalışma alanının TAMAMINI çözüyor, dolayısıyla kırpmasak sunucunun hiç
# kullanmadığı yarım gigabaytlık bir depoyu her derlemede indirirdi.
#
# Hiçbir katmanda `apt` yok. Derleyici imajında `cc` ve `libc-dev` zaten var
# (`rusqlite`'ın gömülü SQLite'ı için gereken tek şey); çalışma imajında da
# `curl` yerine ikilinin kendi `--health-check` yoklaması kullanılıyor.

FROM rust:slim-bookworm AS builder

WORKDIR /src

COPY Cargo.toml Cargo.lock ./
COPY crates/proto crates/proto
COPY crates/doc   crates/doc
COPY crates/sync  crates/sync
# Cihaz rölesinin çerçeve codec'i burada; sunucu ona bağımlı.
COPY crates/rpc   crates/rpc
COPY crates/server crates/server

# Yalnızca gerekmeyen üyeler çıkarılıyor; sürümler kök `Cargo.toml`'da tek
# yerde kalıyor, ikinci bir manifest kopyası tutmuyoruz. `grep` kırpmanın
# hedefi ıskalamadığını doğruluyor: sessizce boşalan bir üye listesi çok
# daha sonra, anlamsız bir bağlantı hatası olarak ortaya çıkardı.
RUN awk '/^members = \[/{p=1} \
         p && /"(crates\/(harness|engine|update|ui|syntax)|apps\/postillion)"/{next} \
         /^\]/{p=0} {print}' Cargo.toml > Cargo.toml.trimmed \
 && mv Cargo.toml.trimmed Cargo.toml \
 && grep -q '"crates/server"' Cargo.toml \
 && grep -q '"crates/rpc"' Cargo.toml

# `--locked` YOK: üye listesi kırpıldığı için cargo kilit dosyasından artık
# kullanılmayan girdileri düşürüyor ve `--locked` bunu hata sayardı. Kalan
# bağımlılıkların sürümleri kilitten geliyor.
#
# Önbellek bağlamaları derlemeler arası kayıt ve nesne önbelleğini koruyor;
# ikili önbellekten dışarı kopyalanıyor çünkü bağlama katmanda kalmıyor.
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/src/target \
    cargo build --release -p postillion-server \
 && cp target/release/postillion-server /usr/local/bin/postillion-server

# Kabuksuz, paket yöneticisiz çalışma imajı: içinde sunucudan başka
# çalıştırılabilir bir şey yok. `cc` değişkesi Rust'ın ihtiyaç duyduğu
# libgcc ve libc'yi taşıyor.
FROM gcr.io/distroless/cc-debian12:nonroot AS runtime

COPY --from=builder /usr/local/bin/postillion-server /usr/local/bin/postillion-server

# Konteyner içinde tüm arayüzler: ağ sınırını Docker ve Coolify'ın vekili
# çiziyor, konteynerin kendisi dışarı yayınlanmıyor.
ENV BIND=0.0.0.0:8787
EXPOSE 8787

ENTRYPOINT ["/usr/local/bin/postillion-server"]
