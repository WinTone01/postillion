import type { HttpContext } from '@adonisjs/core/http'
import { chats, devices } from '#services/registry'
import { presence } from '#services/sync_server'

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

    const ask = (org: string) => presence(org, user.id)
    const [deviceList, chatList] = await Promise.all([devices(user.id, ask), chats(user.id, ask)])

    return view.render('pages/workspace', {
      devices: deviceList.items,
      chats: chatList.items,
      // Canlılık SORULABİLDİ mi — yapılandırma değil, isteğin kendisi.
      // Yapılandırmaya bakmak, reddedilen ya da düşen bir çağrıdan sonra
      // açık duran cihazları "çevrimdışı" göstermek demekti.
      livenessKnown: deviceList.livenessKnown && chatList.livenessKnown,
    })
  }
}
