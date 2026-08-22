import type { HttpContext } from '@adonisjs/core/http'
import vine from '@vinejs/vine'
import ApiToken from '#models/api_token'

const createValidator = vine.compile(
  vine.object({
    name: vine.string().trim().minLength(1).maxLength(64),
  })
)

/**
 * Cihaz jetonları.
 *
 * Ham jeton yalnızca ÜRETİLDİĞİ ANDA, bir kez gösteriliyor. Saklanan tek şey
 * özeti, dolayısıyla sonradan geri getirilemiyor — kullanıcının kopyalamayı
 * kaçırması hâlinde tek yol yeni bir jeton üretmek. Bu bilinçli: özeti
 * saklamak, veritabanını ele geçiren birinin jetonları kullanamaması demek.
 */
export default class TokensController {
  async index({ view, auth, session }: HttpContext) {
    const user = auth.getUserOrFail()
    const tokens = await ApiToken.query().where('user_id', user.id).orderBy('created_at', 'desc')

    return view.render('pages/tokens', {
      tokens,
      // Yeni üretilmiş jeton flash'ta taşınıyor: yenilendiğinde kaybolmalı,
      // sayfada kalıcı olarak durmamalı.
      fresh: session.flashMessages.get('freshToken'),
    })
  }

  async store(ctx: HttpContext) {
    const { request, response, session, auth } = ctx
    const user = auth.getUserOrFail()
    const { name } = await request.validateUsing(createValidator)

    const { raw } = await ApiToken.mint(user.id, name)
    session.flash('freshToken', raw)
    return response.redirect('/tokens')
  }

  async destroy({ params, response, auth }: HttpContext) {
    const user = auth.getUserOrFail()
    // Sorgu kullanıcıyla SINIRLI: kimliği bilinen bir jetonu başkasının
    // silebilmesi, hesabın cihazlarını uzaktan düşürmek olurdu.
    await ApiToken.query().where('id', params.id).andWhere('user_id', user.id).delete()
    return response.redirect('/tokens')
  }
}
