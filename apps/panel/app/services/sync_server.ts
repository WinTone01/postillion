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
