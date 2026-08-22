# Landing sayfası — saf statik dosyalar, derleme adımı yok.
#
# Derleme bağlamı depo KÖKÜ (bkz. `docker-compose.landing.yaml`), bu yüzden
# kopyalanan yollar `apps/landing/` ile başlıyor.
#
# Kendi imajı ve kendi compose'u var; eşitleme sunucusunun yığınına
# eklenmedi. Sebebi bağımsız dağıtım: sayfada bir yazı değiştirmek
# sunucuyu yeniden başlatmamalı, sunucuyu güncellemek de sayfayı
# indirmemeli. İkisi ayrı alan adlarında ve ayrı ömürlerde.

FROM nginx:alpine

# Varsayılan yapılandırma tek satırlık bir sunucu bloğu; kendi
# yapılandırmamız SPA olmayan statik siteye göre ayarlı.
COPY apps/landing/nginx.conf /etc/nginx/conf.d/default.conf
COPY apps/landing/public /usr/share/nginx/html

EXPOSE 80

# Sağlık yoklaması imajın kendi `wget`'iyle: nginx zaten ayakta olduğunda
# cevap veriyor, ayrı bir araç kurmaya gerek yok.
HEALTHCHECK --interval=30s --timeout=5s --start-period=5s --retries=3 \
  CMD wget -q -O /dev/null http://127.0.0.1/ || exit 1
