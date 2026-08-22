import type { HttpContext } from '@adonisjs/core/http'
import type { NextFn } from '@adonisjs/core/types/http'
import type { Authenticators } from '@adonisjs/auth/types'

/**
 * Auth middleware is used authenticate HTTP requests and deny
 * access to unauthenticated users.
 */
export default class AuthMiddleware {
  /**
   * The URL to redirect to, when authentication fails
   */
  redirectTo = '/login'

  async handle(
    ctx: HttpContext,
    next: NextFn,
    options: {
      guards?: (keyof Authenticators)[]
    } = {}
  ) {
    // Adonis giriş sayfasına "Unauthorized access" diye bir hata basıyor.
    // Kullanıcı zaten giriş sayfasında; ona hata göstermek, yanlış bir şey
    // yapmış izlenimi veriyor. Oturum açması gerektiği sayfanın kendisinden
    // zaten belli.
    try {
      await ctx.auth.authenticateUsing(options.guards, { loginRoute: this.redirectTo })
    } catch (error) {
      ctx.session?.flashMessages.clear()
      throw error
    }
    return next()
  }
}
