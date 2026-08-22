import env from '#start/env'
import mail from '@adonisjs/mail/services/main'
import router from '@adonisjs/core/services/router'
import logger from '@adonisjs/core/services/logger'
import { DateTime } from 'luxon'
import type User from '#models/user'

/**
 * Doğrulama e-postası.
 *
 * Bağlantı İMZALI bir URL: sunucuda saklanacak bir jeton yok ve süresi
 * imzanın içinde. Ayrı bir jeton tablosu, temizlenmeyi bekleyen bir sıra
 * daha olurdu.
 */

/**
 * Yeniden gönderim arası en az bekleme.
 *
 * Buton bir SALDIRI ARACI: saldırgan başkasının adresiyle kaydolup butona
 * basmaya devam ederek o kutuyu doldurabilir. Bir dakika, yanlış giden bir
 * gönderimi tekrarlamaya yetiyor ama ardışık basmayı işe yaramaz kılıyor.
 */
export const RESEND_COOLDOWN_SECONDS = 60

/**
 * Postadaki bağlantının mutlak kökü.
 *
 * GÖRELİ OLAMAZ ve bu sessizce bozuluyor: `APP_URL` boşken bağlantı
 * `/verify/1?signature=…` olarak gidiyor, posta istemcisi onu adres
 * çubuğuna koyuyor ve tarayıcı `verify`'ı sunucu adı sanıyor —
 * `https://verify/1`. Kimse hata görmüyor, yalnızca bağlantı açılmıyor.
 *
 * Şemasız yazılmış bir değer (`postillion.net`) aynı şekilde bozuluyor,
 * o yüzden burada tamamlanıyor.
 */
export function appBaseUrl(fallbackOrigin?: string) {
  const configured = env.get('APP_URL', '').trim() || (fallbackOrigin ?? '').trim()
  if (!configured) {
    return ''
  }
  const withScheme = /^https?:\/\//i.test(configured) ? configured : `https://${configured}`
  return withScheme.replace(/\/+$/, '')
}

/**
 * Yapılandırma eksikken gönderim denenmiyor.
 *
 * Kök adres de şart: postayı gönderip içine açılmayan bir bağlantı koymak,
 * hiç göndermemekten daha kötü — kullanıcı gelen kutusuna bakıp bekler.
 */
export function mailConfigured() {
  return (
    env.get('SMTP_HOST', '').length > 0 &&
    env.get('SMTP_USERNAME', '').length > 0 &&
    appBaseUrl().length > 0
  )
}

/**
 * Panelin doğrulama şartı koşup koşmayacağı.
 *
 * Posta gönderilemeyen bir kurulumda doğrulama İSTENMİYOR: istenseydi
 * kimse doğrulama bağlantısını alamayacağı için herkes kalıcı olarak
 * dışarıda kalırdı. Şart, postayı gerçekten gönderebildiğimizde başlıyor.
 */
export function verificationRequired(verified: boolean) {
  return mailConfigured() && !verified
}

/**
 * Doğrulama bağlantısı.
 *
 * 24 saat: elini çabuk tutmayı gerektirmeyecek kadar uzun, çalınmış bir
 * posta kutusunda sonsuza kadar geçerli kalmayacak kadar kısa.
 *
 * İmza yalnızca YOLU kapsıyor, kökü değil; kök yanlışsa bağlantı açılmıyor
 * ama imza yine geçerli kalıyor.
 */
export function verificationUrl(user: User, fallbackOrigin?: string) {
  return router
    .builder()
    .prefixUrl(appBaseUrl(fallbackOrigin))
    .params({ id: user.id })
    .makeSigned('verifyEmail', { expiresIn: '24h' })
}

/** Yeniden göndermek için kalan saniye; 0 ise serbest. */
export function resendCooldownRemaining(user: User, now = DateTime.now()) {
  if (!user.verificationSentAt) {
    return 0
  }
  const elapsed = now.diff(user.verificationSentAt, 'seconds').seconds
  return Math.max(0, Math.ceil(RESEND_COOLDOWN_SECONDS - elapsed))
}

/**
 * Doğrulama postasını gönderir.
 *
 * Hata YUTULUYOR ve loglanıyor: e-posta sağlayıcısının erişilemez olması
 * kaydın başarısız olmasına yol açmamalı — kullanıcı hesabını almış
 * durumda ve postayı yeniden isteyebiliyor.
 */
export async function sendVerification(user: User, fallbackOrigin?: string) {
  if (!mailConfigured()) {
    logger.warn(
      { appUrl: appBaseUrl(fallbackOrigin) },
      'posta yapılandırılmadı (SMTP_HOST/SMTP_USERNAME/APP_URL); doğrulama postası gönderilmedi'
    )
    return false
  }

  const url = verificationUrl(user, fallbackOrigin)
  try {
    await mail.send((message) => {
      message
        .to(user.email)
        .subject('Postillion — verify your e-mail address')
        .htmlView('emails/verify', { url })
    })
  } catch (error) {
    logger.error({ err: error }, 'doğrulama postası gönderilemedi')
    return false
  }

  // Damga gönderim BAŞARILI olduğunda vuruluyor: başarısız bir denemenin
  // ardından kullanıcıyı bir dakika bekletmek, onu bizim arızamız için
  // cezalandırmak olurdu.
  user.verificationSentAt = DateTime.now()
  await user.save()
  return true
}
