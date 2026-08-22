import { defineConfig } from '@adonisjs/shield'

const shieldConfig = defineConfig({
  /**
   * Configure CSP policies for your app. Refer documentation
   * to learn more
   */
  csp: {
    // Açık: panelde kullanıcıdan gelen metin (sohbet içeriği) gösterilecek ve
    // enjekte edilen bir script'in çalışabilmesi ile çalışamaması arasındaki
    // fark bu başlık.
    enabled: true,
    directives: {
      defaultSrc: [`'self'`],
      // Turnstile widget'ı Cloudflare'den yükleniyor.
      scriptSrc: [`'self'`, 'https://challenges.cloudflare.com'],
      frameSrc: [`'self'`, 'https://challenges.cloudflare.com'],
      // Vite geliştirmede stilleri enjekte ediyor; üretimde dosyadan geliyor.
      styleSrc: [`'self'`, `'unsafe-inline'`],
      imgSrc: [`'self'`, 'data:'],
      // Yalnızca kendi kaynağımız yeterli: tarayıcı eşitleme sunucusuna HİÇ
      // gitmiyor. Gitseydi sunucu jetonunun tarayıcıya verilmesi gerekirdi ve
      // o jeton kullanıcının bütün odalarına açılıyor — panel vekillik ediyor.
      connectSrc: [`'self'`],
      objectSrc: [`'none'`],
      baseUri: [`'self'`],
      formAction: [`'self'`],
      frameAncestors: [`'none'`],
    },
    reportOnly: false,
  },

  /**
   * Configure CSRF protection options. Refer documentation
   * to learn more
   */
  csrf: {
    enabled: true,
    exceptRoutes: [],
    enableXsrfCookie: false,
    methods: ['POST', 'PUT', 'PATCH', 'DELETE'],
  },

  /**
   * Control how your website should be embedded inside
   * iFrames
   */
  xFrame: {
    enabled: true,
    action: 'DENY',
  },

  /**
   * Force browser to always use HTTPS
   */
  hsts: {
    enabled: true,
    maxAge: '180 days',
  },

  /**
   * Disable browsers from sniffing the content type of a
   * response and always rely on the "content-type" header.
   */
  contentTypeSniffing: {
    enabled: true,
  },
})

export default shieldConfig
