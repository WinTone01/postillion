import env from '#start/env'

/**
 * Eşitleme sunucusunun HTTP ucu.
 *
 * Panel çoğu şeyi doğrudan veritabanından okuyor ama iki şey orada YOK:
 * canlı presence (bellekte) ve materyalize transkript (satırlar opak loro
 * güncellemesi). İkisi için sunucuya gidiliyor.
 */

function baseUrl() {
  return env.get('POSTILLION_SERVER_URL', '').replace(/\/+$/, '')
}

function token() {
  return env.get('POSTILLION_SERVER_TOKEN', '')
}

/** Sunucu yapılandırılmış mı — değilse panel liste gösterir, canlılık gösteremez. */
export function configured() {
  return baseUrl().length > 0 && token().length > 0
}

/**
 * Panelin sunucuya sunduğu jeton İŞLETMECİNİN anahtarı, kullanıcının değil —
 * kimliği `SHARED_USER`. Odalar ise kullanıcıya ait ve sahiplik denetimi o
 * kimliği içeri almıyor: cihazlar çevrimdışı, transkript "ulaşılamadı"
 * görünüyordu. Bu başlık isteğin kimin adına yapıldığını söylüyor ve sunucu
 * onu yalnızca paylaşılan jetonla kabul ediyor.
 */
const ACT_AS = 'x-postillion-act-as'

async function get(path: string, userId: number): Promise<unknown | null> {
  if (!configured()) {
    return null
  }
  try {
    const response = await fetch(`${baseUrl()}${path}`, {
      headers: {
        authorization: `Bearer ${token()}`,
        [ACT_AS]: String(userId),
      },
      // Sunucu erişilemezse panel AÇILMAYA devam etmeli: liste
      // veritabanından geliyor ve canlılık bilgisi olmadan da işe yarıyor.
      signal: AbortSignal.timeout(5000),
    })
    return response.ok ? await response.json() : null
  } catch {
    return null
  }
}

/**
 * Bir kayıt odasındaki canlı cihaz kimlikleri; ulaşılamazsa `null`.
 *
 * "Ulaşılamadı" ile "kimse çevrimiçi değil" AYRI olmak zorunda: ikisini
 * birleştirmek, açık duran bir cihazı kapalı göstermek demek — kullanıcıya
 * yanlış bilgi. Panel bunları tam olarak böyle gösteriyordu.
 */
export async function presence(org: string, userId: number): Promise<string[] | null> {
  const body = (await get(`/registry/${encodeURIComponent(org)}/presence`, userId)) as {
    devices?: Record<string, number>
  } | null
  return body === null ? null : Object.keys(body.devices ?? {})
}

/** Transkriptteki bir parça. Sunucunun `MessagePart` etiketli birleşimi. */
export interface Part {
  kind: string
  text?: string
  message?: string
  call?: { kind?: string; command?: string; path?: string; pattern?: string; name?: string }
  resolved?: boolean
  isError?: boolean
}

export interface Message {
  id: string
  role: 'user' | 'assistant' | 'system'
  parts: Part[]
  createdAt: number
  deviceId: string
}

/**
 * Bir sohbetin materyalize edilmiş mesajları.
 *
 * Satırlar opak loro güncellemesi; birleştirmeyi SUNUCU yapıyor. Bu yüzden
 * transkript bilgisayar kapalıyken de okunuyor — host'a soruyor olsaydık
 * kapalı bir dizüstünde cevap veren kimse olmazdı.
 *
 * `null` "ulaşılamadı" demek; boş dizi "sohbet boş". İkisini ayırmak
 * gerekiyor, yoksa bağlantı arızası boş bir sohbet gibi görünürdü.
 */
export interface Transcript {
  headSeq: number
  /** `null` "değişmedi" demek — `since` verildiğinde gelebiliyor. */
  messages: Message[] | null
}

export async function transcript(
  chatId: string,
  userId: number,
  since?: number
): Promise<Transcript | null> {
  const query = since === undefined ? '' : `?since=${since}`
  const body = (await get(`/chat2/${encodeURIComponent(chatId)}/messages${query}`, userId)) as {
    headSeq?: number
    messages?: Message[]
    unchanged?: boolean
  } | null
  if (!body) {
    return null
  }
  return {
    headSeq: body.headSeq ?? 0,
    // "Değişmedi" ile "boş sohbet" AYRI: ilkinde ekrandakini korumak
    // gerekiyor, ikincisinde temizlemek.
    messages: body.unchanged ? null : (body.messages ?? []),
  }
}
