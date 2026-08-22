import db from '@adonisjs/lucid/services/db'

/**
 * Kayıt satırlarını okur — cihaz ve sohbet listesi.
 *
 * Bu satırlar sunucunun `registry_rows` tablosunda düz JSON olarak duruyor,
 * yani liste için CRDT çalıştırmaya gerek yok. Sohbetin İÇERİĞİ farklı: o opak
 * loro güncellemesi ve sunucu materyalize ediyor (`GET /chat2/{id}/messages`).
 *
 * Liste bilgisayar KAPALIYKEN de görünüyor — satırlar sunucuda, cihazda değil.
 */

interface Row {
  id: string
  fields: Record<string, unknown>
}

export interface Device {
  id: string
  name: string
  platform: string
  /** Açılış/kapanış damgası — canlılık ÖLÇÜSÜ DEĞİL, bkz. `online`. */
  lastSeenAt: number | null
  online: boolean
}

export interface Chat {
  id: string
  title: string
  deviceId: string | null
  cwd: string | null
  branch: string | null
  lastMessageAt: number | null
  /** `cwd · branch` — şablonda birleştirmek yerine burada, tek yerde. */
  location: string
  /** Sohbeti tutan cihaz şu an açık mı — yazma bu şarta bağlı. */
  deviceOnline: boolean
}

/** Kullanıcının sahiplendiği kayıt odaları. */
async function orgsOf(userId: number): Promise<string[]> {
  const rows = await db
    .from('room_owners')
    .select('room')
    .where('scope', 'registry')
    .andWhere('user_id', userId)
  return rows.map((r: { room: string }) => r.room)
}

async function rowsOf(userId: number, kind: string): Promise<Row[]> {
  const orgs = await orgsOf(userId)
  if (orgs.length === 0) {
    return []
  }
  // Sorgu kullanıcının odalarıyla SINIRLI. Sunucu da aynı denetimi yapıyor
  // ama panel veritabanına doğrudan gidiyor ve onu atlıyor — izolasyon
  // burada AYRICA kurulmak zorunda.
  const rows = await db
    .from('registry_rows')
    .select('id', 'fields')
    .whereIn('org', orgs)
    .andWhere('kind', kind)
    .andWhere('deleted', false)
  return rows as Row[]
}

function str(fields: Record<string, unknown>, key: string): string | null {
  const value = fields[key]
  return typeof value === 'string' && value.length > 0 ? value : null
}

function ms(fields: Record<string, unknown>, key: string): number | null {
  const value = fields[key]
  return typeof value === 'number' ? value : null
}

/**
 * Çevrimiçi cihazlar.
 *
 * Kayıt satırlarındaki `lastSeenAt` bu soruyu CEVAPLAMIYOR: o yalnızca açılış
 * ve kapanışta yazılıyor, dolayısıyla açık duran bir cihaz orada saatler
 * öncesinde görünüyor. Canlı atışlar sunucunun belleğinde ve oradan
 * okunuyor.
 */
async function onlineIds(userId: number, fetchPresence: PresenceFetcher): Promise<Set<string>> {
  const orgs = await orgsOf(userId)
  const ids = new Set<string>()
  for (const org of orgs) {
    for (const id of await fetchPresence(org)) {
      ids.add(id)
    }
  }
  return ids
}

/** Bir odadaki canlı cihaz kimliklerini döndürür. */
export type PresenceFetcher = (org: string) => Promise<string[]>

export async function devices(userId: number, presence: PresenceFetcher): Promise<Device[]> {
  const [rows, online] = await Promise.all([
    rowsOf(userId, 'devices'),
    onlineIds(userId, presence),
  ])

  return rows
    .map((row) => ({
      id: row.id,
      // Adsız cihaz listede boş bir satır olarak görünmemeli.
      name: str(row.fields, 'name') ?? row.id,
      platform: str(row.fields, 'platform') ?? 'bilinmiyor',
      lastSeenAt: ms(row.fields, 'lastSeenAt'),
      online: online.has(row.id),
    }))
    // Çevrimiçi olanlar üstte: panelin işi onlarla.
    .sort((a, b) => Number(b.online) - Number(a.online) || (b.lastSeenAt ?? 0) - (a.lastSeenAt ?? 0))
}

export async function chats(userId: number, presence: PresenceFetcher): Promise<Chat[]> {
  const [rows, online] = await Promise.all([
    rowsOf(userId, 'chats'),
    onlineIds(userId, presence),
  ])

  return rows
    .filter((row) => row.fields.archived !== true)
    .map((row) => {
      const deviceId = str(row.fields, 'deviceId')
      const cwd = str(row.fields, 'cwd')
      const branch = str(row.fields, 'branch')
      return {
        id: row.id,
        title: str(row.fields, 'title') ?? 'Başlıksız',
        deviceId,
        cwd,
        branch,
        location: [cwd, branch].filter(Boolean).join(' · '),
        lastMessageAt: ms(row.fields, 'lastMessageAt'),
        deviceOnline: deviceId !== null && online.has(deviceId),
      }
    })
    .sort((a, b) => (b.lastMessageAt ?? 0) - (a.lastMessageAt ?? 0))
}
