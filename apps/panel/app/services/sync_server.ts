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

async function get(path: string): Promise<unknown | null> {
  if (!configured()) {
    return null
  }
  try {
    const response = await fetch(`${baseUrl()}${path}`, {
      headers: { authorization: `Bearer ${token()}` },
      // Sunucu erişilemezse panel AÇILMAYA devam etmeli: liste
      // veritabanından geliyor ve canlılık bilgisi olmadan da işe yarıyor.
      signal: AbortSignal.timeout(5000),
    })
    return response.ok ? await response.json() : null
  } catch {
    return null
  }
}

/** Bir kayıt odasındaki canlı cihaz kimlikleri. */
export async function presence(org: string): Promise<string[]> {
  const body = (await get(`/registry/${encodeURIComponent(org)}/presence`)) as
    | { devices?: Record<string, number> }
    | null
  return Object.keys(body?.devices ?? {})
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
export async function transcript(chatId: string): Promise<Message[] | null> {
  const body = (await get(`/chat2/${encodeURIComponent(chatId)}/messages`)) as
    | { messages?: Message[] }
    | null
  return body ? (body.messages ?? []) : null
}
