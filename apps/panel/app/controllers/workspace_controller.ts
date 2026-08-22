import type { HttpContext } from '@adonisjs/core/http'
import { chats, devices } from '#services/registry'
import { configured, presence } from '#services/sync_server'

/**
 * Panelin ana ekranı: cihazlar ve sohbetler.
 *
 * Liste veritabanından, canlılık sunucudan geliyor. Sunucuya ulaşılamazsa
 * liste yine gösteriliyor — geçmişi görmek için cihazın açık olması
 * gerekmiyor ve bunu bir bağlantı arızası yüzünden gizlemek yanlış olurdu.
 */
export default class WorkspaceController {
  async index({ view, auth }: HttpContext) {
    const user = auth.getUserOrFail()

    const [deviceList, chatList] = await Promise.all([
      devices(user.id, presence),
      chats(user.id, presence),
    ])

    return view.render('pages/workspace', {
      devices: deviceList,
      chats: chatList,
      // Sunucu ayarlı değilse canlılık hiç bilinmiyor; arayüz "çevrimdışı"
      // demek yerine susmalı, yoksa açık bir cihazı kapalı gösterirdi.
      livenessKnown: configured(),
    })
  }
}
