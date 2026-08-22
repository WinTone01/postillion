import type { HttpContext } from '@adonisjs/core/http'
import { DateTime } from 'luxon'
import User from '#models/user'
import { mailConfigured, resendCooldownRemaining, sendVerification } from '#services/mailer'

/**
 * `APP_URL` ayarlanmamışsa postadaki kökü isteğin kendisinden alıyoruz.
 *
 * Değişkeni unutmak bağlantıyı göreli bırakıyor ve göreli bir bağlantı
 * postada işe yaramıyor — tarayıcı ilk parçayı sunucu adı sanıyor. Ters
 * vekilin arkasında doğru değerler için `trustProxy` gerekiyor (config/app.ts).
 */
function requestOrigin(request: HttpContext['request']) {
  return `${request.protocol()}://${request.host()}`
}

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
      // Butonu erken basılamaz yapmak nezaket; şartı uygulayan `resend`.
      cooldown: resendCooldownRemaining(user),
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
  async resend({ request, response, session, auth }: HttpContext) {
    const user = auth.getUserOrFail()
    if (user.isVerified) {
      return response.redirect('/app')
    }

    // Bekleme süresi dolmadan gönderim YOK. Kontrol burada değil de yalnızca
    // arayüzde olsaydı, formu doğrudan POST etmek onu tümüyle atlardı.
    const wait = resendCooldownRemaining(user)
    if (wait > 0) {
      session.flash('errorsBag', {
        cooldown: `Please wait ${wait} more second${wait === 1 ? '' : 's'} before requesting another e-mail.`,
      })
      return response.redirect().back()
    }

    const sent = await sendVerification(user, requestOrigin(request))
    session.flash(
      sent ? 'notice' : 'errorsBag',
      sent ? 'Verification e-mail sent.' : { mail: 'The e-mail could not be sent.' }
    )
    return response.redirect().back()
  }
}
