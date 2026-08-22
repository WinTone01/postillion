import type { HttpContext } from '@adonisjs/core/http'
import db from '@adonisjs/lucid/services/db'
import { configured, presence, transcript } from '#services/sync_server'
import { chats } from '#services/registry'
import { RelayError, sendPrompt } from '#services/device_rpc'
import vine from '@vinejs/vine'

const sendValidator = vine.compile(
  vine.object({
    prompt: vine.string().trim().minLength(1).maxLength(20_000),
  })
)

/**
 * Bir sohbetin transkripti.
 *
 * Okumak cihazdan BAĞIMSIZ: satırları sunucu birleştiriyor. Yazmak değil —
 * ajan cihazda çalışıyor ve o kapalıysa gönderilecek bir yer yok.
 */
export default class ChatController {
  async show({ params, view, auth, response }: HttpContext) {
    const user = auth.getUserOrFail()

    // Sohbetin bu kullanıcıya ait olduğunu ÖNCE doğruluyoruz. Sunucu da
    // doğruluyor ama panel oraya kullanıcının değil kendi jetonuyla gidiyor,
    // dolayısıyla o denetim burada bir şey kanıtlamıyor.
    const owned = await this.owns(user.id, params.id)
    if (!owned) {
      return response.notFound('Sohbet bulunamadı')
    }

    const messages = await transcript(params.id)
    const list = await chats(user.id, presence)
    const chat = list.find((c) => c.id === params.id)

    return view.render('pages/chat', {
      chat,
      messages,
      // `null` ile boş sohbeti ayırmak gerekiyor: ilki arıza, ikincisi
      // normal durum ve arayüzde farklı görünmeliler.
      unreachable: messages === null,
      livenessKnown: configured(),
    })
  }

  /**
   * Sohbete mesaj yazar.
   *
   * Okumanın aksine bu cihaza BAĞLI: ajan orada çalışıyor. Cihaz kapalıysa
   * röle `host_offline` döndürüyor ve kullanıcı zaman aşımı beklemek yerine
   * anında öğreniyor.
   */
  async send({ params, request, response, session, auth }: HttpContext) {
    const user = auth.getUserOrFail()
    if (!(await this.owns(user.id, params.id))) {
      return response.notFound('Sohbet bulunamadı')
    }

    const { prompt } = await request.validateUsing(sendValidator)
    const chat = (await chats(user.id, presence)).find((c) => c.id === params.id)
    if (!chat?.deviceId) {
      session.flash('errorsBag', { device: 'Bu sohbetin cihazı bilinmiyor.' })
      return response.redirect().back()
    }

    try {
      await sendPrompt(chat.deviceId, params.id, prompt, chat.cwd ?? '')
    } catch (error) {
      // Röle hataları kullanıcıya OLDUĞU GİBİ gösteriliyor ("Cihaz
      // çevrimdışı" gibi); genel bir "gönderilemedi" ne yapacağını
      // söylemezdi.
      session.flash('errorsBag', {
        send: error instanceof RelayError ? error.message : 'Mesaj gönderilemedi.',
      })
      session.flashOnly(['prompt'])
    }
    return response.redirect().back()
  }

  /**
   * Sohbet kullanıcının bir kayıt odasında mı.
   *
   * `registry_rows` üzerinden: sohbetin satırı kullanıcının sahiplendiği bir
   * odada duruyorsa onundur.
   */
  private async owns(userId: number, chatId: string) {
    const row = await db
      .from('registry_rows as r')
      .join('room_owners as o', (join) => {
        join.on('o.room', '=', 'r.org').andOnVal('o.scope', 'registry')
      })
      .where('r.kind', 'chats')
      .andWhere('r.id', chatId)
      .andWhere('o.user_id', userId)
      .first()
    return row !== null && row !== undefined
  }
}
