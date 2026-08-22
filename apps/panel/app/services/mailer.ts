import env from '#start/env'
import mail from '@adonisjs/mail/services/main'
import router from '@adonisjs/core/services/router'
import logger from '@adonisjs/core/services/logger'
import type User from '#models/user'

/**
 * Doğrulama e-postası.
 *
 * Bağlantı İMZALI bir URL: sunucuda saklanacak bir jeton yok ve süresi
 * imzanın içinde. Ayrı bir jeton tablosu, temizlenmeyi bekleyen bir sıra
 * daha olurdu.
 */

/** Yapılandırma eksikken gönderim denenmiyor. */
export function mailConfigured() {
  return env.get('SMTP_HOST', '').length > 0 && env.get('SMTP_USERNAME', '').length > 0
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
 */
export function verificationUrl(user: User) {
  return router
    .builder()
    .prefixUrl(env.get('APP_URL', ''))
    .params({ id: user.id })
    .makeSigned('verifyEmail', { expiresIn: '24h' })
}

/**
 * Doğrulama postasını gönderir.
 *
 * Hata YUTULUYOR ve loglanıyor: e-posta sağlayıcısının erişilemez olması
 * kaydın başarısız olmasına yol açmamalı — kullanıcı hesabını almış
 * durumda ve postayı yeniden isteyebiliyor.
 */
export async function sendVerification(user: User) {
  if (!mailConfigured()) {
    logger.warn('SMTP yapılandırılmadı; doğrulama postası gönderilmedi')
    return false
  }

  const url = verificationUrl(user)
  try {
    await mail.send((message) => {
      message
        .to(user.email)
        .subject('Postillion — e-postanızı doğrulayın')
        .htmlView('emails/verify', { url })
    })
    return true
  } catch (error) {
    logger.error({ err: error }, 'doğrulama postası gönderilemedi')
    return false
  }
}
