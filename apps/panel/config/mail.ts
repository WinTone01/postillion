import env from '#start/env'
import { defineConfig, transports } from '@adonisjs/mail'

/**
 * E-posta gönderimi — Mailtrap.
 *
 * Mailtrap SMTP konuşuyor, dolayısıyla ayrı bir sürücüye gerek yok; sunucu
 * adresi hangi Mailtrap ürününü kullandığınızı belirliyor (sandbox testte
 * postayı yakalıyor, sending gerçekten gönderiyor).
 *
 * Anahtarlar yoksa gönderim DENENMİYOR (bkz. `app/services/mailer.ts`):
 * geliştirme ve testte Mailtrap hesabı olmayabilir ve kayıt akışının bu
 * yüzden hiç çalışmaması yanlış olurdu.
 */
const mailConfig = defineConfig({
  default: 'smtp',

  from: {
    address: env.get('MAIL_FROM_ADDRESS', 'noreply@postillion.local'),
    name: env.get('MAIL_FROM_NAME', 'Postillion'),
  },

  mailers: {
    smtp: transports.smtp({
      host: env.get('SMTP_HOST', 'localhost'),
      port: env.get('SMTP_PORT', '587'),
      // Mailtrap TLS'i STARTTLS ile veriyor; `secure: true` 465 içindir ve
      // 587'de el sıkışmayı bozardı.
      secure: false,
      auth: {
        type: 'login',
        user: env.get('SMTP_USERNAME', ''),
        pass: env.get('SMTP_PASSWORD', ''),
      },
    }),
  },
})

export default mailConfig

declare module '@adonisjs/mail/types' {
  export interface MailersList extends InferMailers<typeof mailConfig> {}
}
