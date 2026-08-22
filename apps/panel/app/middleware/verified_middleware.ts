import type { HttpContext } from '@adonisjs/core/http'
import type { NextFn } from '@adonisjs/core/types/http'
import { verificationRequired } from '#services/mailer'

/**
 * Doğrulanmamış hesabı panelden uzak tutar.
 *
 * Giriş yapmayı engellemiyoruz — kullanıcının kendi durumunu görebilmesi ve
 * postayı yeniden isteyebilmesi gerekiyor. Engellenen şey panelin kendisi:
 * oradan gerçek makinelere komut gidiyor.
 */
export default class VerifiedMiddleware {
  async handle(ctx: HttpContext, next: NextFn) {
    const user = ctx.auth.user
    if (user && verificationRequired(user.isVerified)) {
      return ctx.response.redirect('/verify')
    }
    return next()
  }
}
