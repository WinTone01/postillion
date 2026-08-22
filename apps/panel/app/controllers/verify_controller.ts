import type { HttpContext } from '@adonisjs/core/http'
import { DateTime } from 'luxon'
import User from '#models/user'
import { mailConfigured, sendVerification } from '#services/mailer'

/**
 * E-posta doğrulama.
 *
 * Bağlantı imzalı: gövdesinde saklanacak bir sır yok ve süresi imzada.
 */
export default class VerifyController {
  /** Doğrulanmamış kullanıcıya gösterilen bekleme ekranı. */
  async notice({ view, auth }: HttpContext) {
    const user = auth.getUserOrFail()
    return view.render('pages/auth/verify', {
      email: user.email,
      // Posta hiç gönderilemiyorsa kullanıcı boşuna beklememelidir.
      mailConfigured: mailConfigured(),
    })
  }

  /** İmzalı bağlantı — hesabı doğrular. */
  async verify({ params, request, response, session }: HttpContext) {
    // İmza doğrulaması İLK: geçersizse kullanıcıyı hiç aramıyoruz, yoksa
    // kimlik numarası deneyerek hangi hesapların var olduğu öğrenilebilirdi.
    if (!request.hasValidSignature()) {
      session.flash('errorsBag', {
        link: 'That link is invalid or has expired. Request a new one.',
      })
      return response.redirect('/login')
    }

    const user = await User.find(params.id)
    if (!user) {
      return response.redirect('/login')
    }

    // Zaten doğrulanmışsa damgayı EZMİYORUZ: bağlantıya ikinci kez tıklamak
    // doğrulama tarihini bugüne çekmemeli.
    if (!user.isVerified) {
      user.emailVerifiedAt = DateTime.now()
      await user.save()
    }

    session.flash('notice', 'Your e-mail address is verified.')
    return response.redirect('/login')
  }

  /** Postayı yeniden gönderir. */
  async resend({ response, session, auth }: HttpContext) {
    const user = auth.getUserOrFail()
    if (user.isVerified) {
      return response.redirect('/app')
    }

    const sent = await sendVerification(user)
    session.flash(
      sent ? 'notice' : 'errorsBag',
      sent ? 'Verification e-mail sent.' : { mail: 'The e-mail could not be sent.' }
    )
    return response.redirect().back()
  }
}
