import env from '#start/env'
import app from '@adonisjs/core/services/app'
import { Secret } from '@adonisjs/core/helpers'
import proxyAddr from 'proxy-addr'
import { defineConfig } from '@adonisjs/core/http'

/**
 * The app key is used for encrypting cookies, generating signed URLs,
 * and by the "encryption" module.
 *
 * The encryption module will fail to decrypt data if the key is lost or
 * changed. Therefore it is recommended to keep the app key secure.
 */
export const appKey = new Secret(env.get('APP_KEY'))

/**
 * The configuration settings used by the HTTP server
 */
export const http = defineConfig({
  generateRequestId: true,

  /**
   * Ters vekilin `X-Forwarded-*` başlıkları.
   *
   * Varsayılan yalnızca loopback'e güveniyor, ama Coolify'ın vekili docker
   * ağından geliyor — özel bir adres. Güvenilmediğinde `request.protocol()`
   * her zaman `http` diyor ve `secure` çerezler ile posta bağlantısının
   * şeması yanlış çıkıyor.
   *
   * Konteynere yalnızca vekil ulaşabildiği için özel aralıklara güvenmek
   * burada sahtecilik penceresi açmıyor.
   */
  //
  // Virgüllü tek dize İŞE YARAMIYOR: Adonis onu bölmeden proxy-addr'e
  // veriyor ve uygulama "invalid IP address" ile açılışta düşüyor.
  trustProxy: proxyAddr.compile(['loopback', 'linklocal', 'uniquelocal']),
  // Tarayıcı formları yalnızca GET ve POST gönderebiliyor. Bu olmadan
  // "iptal et" için ya POST rotası açmak (yıkıcı bir işlemi POST'a koymak)
  // ya da JavaScript'e bağımlı olmak gerekirdi.
  allowMethodSpoofing: true,

  /**
   * Enabling async local storage will let you access HTTP context
   * from anywhere inside your application.
   */
  useAsyncLocalStorage: false,

  /**
   * Manage cookies configuration. The settings for the session id cookie are
   * defined inside the "config/session.ts" file.
   */
  cookie: {
    domain: '',
    path: '/',
    maxAge: '2h',
    httpOnly: true,
    secure: app.inProduction,
    sameSite: 'lax',
  },
})
