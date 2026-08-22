import env from '#start/env'
import { defineConfig, services } from 'adonis-captcha-guard'
// `InferCaptchaProviders` alt yolda; paketin ürettiği yapılandırma onu içe
// aktarmayı atlıyor ve tip denetimi düşüyor.
import type { InferCaptchaProviders } from 'adonis-captcha-guard/types'

/**
 * Yalnızca Turnstile.
 *
 * reCAPTCHA da destekleniyor ama alan adı zaten Cloudflare'in arkasında;
 * ikinci bir sağlayıcıyı yapılandırmada tutmak, kullanılmayan bir anahtar
 * çiftini güncel tutma yükümlülüğü demekti.
 *
 * Anahtarlar boş olabilir: geliştirme ve test kurulumunda Turnstile hesabı
 * olmayabilir. Denetleyici o durumda doğrulamayı atlıyor — bkz.
 * `AuthController.captchaOk`.
 */
const captchaConfig = defineConfig({
  turnstile: services.turnstile({
    siteKey: env.get('TURNSTILE_SITE_KEY', ''),
    secret: env.get('TURNSTILE_SECRET', ''),
  }),
})

export default captchaConfig

// Paketin ÜRETTİĞİ stub burada '@adonisjs/core/types' modülünü genişletiyordu
// ama `use()` paketin KENDİ `CaptchaProviders` arayüzüne bakıyor
// (`src/types.d.ts`), dolayısıyla oradaki boş kalıyor ve `use()` `never`
// döndürüyordu.
declare module 'adonis-captcha-guard/types' {
  interface CaptchaProviders extends InferCaptchaProviders<typeof captchaConfig> {}
}
