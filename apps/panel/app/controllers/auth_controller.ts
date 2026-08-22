import type { HttpContext } from '@adonisjs/core/http'
import { DateTime } from 'luxon'
import env from '#start/env'
import User from '#models/user'
import { loginValidator, registerValidator } from '#validators/auth'
import { mailConfigured, sendVerification, verificationRequired } from '#services/mailer'

/**
 * Kayıt ve giriş.
 *
 * Captcha doğrulaması her iki formda da var. Girişte de olmasının sebebi
 * parola deneme saldırıları: kayıt formunu korumak, giriş formunu açık
 * bırakmak korumanın yarısını atlamak olurdu.
 */
/**
 * Captcha jetonunun form alan adı.
 *
 * adonis-captcha-guard'ın Turnstile sağlayıcısı bu adı sabitlemiş
 * (`tokenParamName`); widget'ı ona uydurmak, paketin doğrulamasını
 * atlayıp jetonu elle POST etmekten daha az kod.
 */
export const CAPTCHA_TOKEN_FIELD = 'token'

export default class AuthController {
  private siteKey() {
    return env.get('TURNSTILE_SITE_KEY', '')
  }

  showRegister({ view }: HttpContext) {
    return view.render('pages/auth/register', { turnstileSiteKey: this.siteKey() })
  }

  showLogin({ view }: HttpContext) {
    return view.render('pages/auth/login', { turnstileSiteKey: this.siteKey() })
  }

  /**
   * Captcha jetonunu sunucu tarafında doğrular.
   *
   * Widget'ın sayfada görünmesi hiçbir şey kanıtlamıyor — jetonu Cloudflare'e
   * sormadan kabul etmek korumanın tamamını atlamak olur.
   *
   * Site anahtarı tanımlı değilken atlanıyor: geliştirme ve test kurulumunda
   * Turnstile hesabı olmayabilir ve bu durumda kayıt hiç çalışmazdı.
   */
  private async captchaOk(ctx: HttpContext) {
    if (!this.siteKey()) {
      return true
    }
    // Jetonun alan adı `token` — Turnstile'ın varsayılanı DEĞİL.
    // adonis-captcha-guard `request.input('token')` okuyor ve bulamazsa
    // 400 fırlatıyor, dolayısıyla widget da o adla göndermek zorunda
    // (`data-response-field-name`, bkz. login.edge). İki taraf ayrı düşerse
    // captcha çözülmüş olsa bile giriş 400 ile kırılıyor.
    if (!ctx.request.input(CAPTCHA_TOKEN_FIELD)) {
      return false
    }
    const result = await ctx.captcha.use('turnstile').validate()
    return result.success
  }

  async register(ctx: HttpContext) {
    const { request, response, session, auth } = ctx

    if (!(await this.captchaOk(ctx))) {
      session.flash('errorsBag', { captcha: 'Verification failed, please try again.' })
      session.flashOnly(['email'])
      return response.redirect().back()
    }

    const payload = await request.validateUsing(registerValidator)

    // Aynı e-posta ikinci kez: veritabanının tekil kısıtı zaten engelliyor ama
    // hata mesajı kullanıcıya bir şey anlatmazdı.
    const existing = await User.findBy('email', payload.email)
    if (existing) {
      session.flash('errorsBag', { email: 'That e-mail address is already registered.' })
      session.flashOnly(['email'])
      return response.redirect().back()
    }

    const user = await User.create(payload)

    // Posta gönderimi yapılandırılmamışsa hesap DOĞRU KABUL EDİLİYOR: aksi
    // halde Mailtrap'siz bir kurulumda kimse panele giremezdi.
    if (!mailConfigured()) {
      user.emailVerifiedAt = DateTime.now()
      await user.save()
    }

    await auth.use('web').login(user)

    // Gönderim kaydın ARDINDAN: postacı erişilemez olsa bile hesap duruyor ve
    // kullanıcı bekleme ekranından yeniden isteyebiliyor.
    if (!user.isVerified) {
      await sendVerification(user)
      return response.redirect('/verify')
    }

    return response.redirect('/app')
  }

  async login(ctx: HttpContext) {
    const { request, response, session, auth } = ctx

    if (!(await this.captchaOk(ctx))) {
      session.flash('errorsBag', { captcha: 'Verification failed, please try again.' })
      session.flashOnly(['email'])
      return response.redirect().back()
    }

    const { email, password } = await request.validateUsing(loginValidator)

    try {
      const user = await User.verifyCredentials(email, password)
      await auth.use('web').login(user)
      // Doğrulanmamış hesap giriş YAPABİLİYOR ama panele değil bekleme
      // ekranına iniyor: kendi durumunu görmesi ve postayı yeniden isteyebilmesi
      // gerekiyor.
      return response.redirect(verificationRequired(user.isVerified) ? '/verify' : '/app')
    } catch {
      // Tek ve AYRIM YAPMAYAN mesaj: "böyle bir kullanıcı yok" ile "parola
      // yanlış"ı ayırmak, hangi e-postaların kayıtlı olduğunu sızdırırdı.
      session.flash('errorsBag', { credentials: 'Wrong e-mail address or password.' })
      session.flashOnly(['email'])
      return response.redirect().back()
    }
  }

  async logout({ response, auth }: HttpContext) {
    await auth.use('web').logout()
    return response.redirect('/login')
  }
}
