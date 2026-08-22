import { randomUUID } from 'node:crypto'
import env from '#start/env'
import { decode, encode } from '#services/device_frame'

/**
 * Cihaz rölesi üzerinden tek bir RPC çağrısı.
 *
 * Panel eşitleme protokolünü konuşmuyor; yalnızca bu röleye `role=client`
 * olarak bağlanıp motorun zaten sunduğu RPC'yi çağırıyor. `QueueCommand`
 * `targetDeviceId` ile uzak cihaza yönlendirilebilir olduğu için
 * (`crates/engine/src/rpc.rs`, `forwardable`) mesaj yazmanın yolu bu.
 *
 * Bağlantı çağrı BAŞINA açılıyor ve kapanıyor. Kalıcı bir bağlantı daha
 * verimli olurdu ama panelde gönderim seyrek ve açık tutulan bir soket,
 * sürecin ömrü boyunca yönetilmesi gereken bir durum demek.
 */

const RPC_KIND = 'rpc'
/** Bağlantı + cevap için üst sınır. */
const TIMEOUT_MS = 15_000

export class RelayError extends Error {}

/**
 * Röle adresi.
 *
 * `actAs` başlık DEĞİL sorgu parametresi: panelin sunduğu jeton
 * işletmecinin ve kimliği `SHARED_USER`, cihaz odası ise kullanıcıya ait —
 * kim adına bağlandığımızı söylemeden sahiplik denetimi 403 veriyor. Başlık
 * kullanılamıyor çünkü Node'un yerleşik `WebSocket`'i el sıkışmaya başlık
 * koymaya izin vermiyor; jetonun da burada olmasının sebebi aynı. Sunucu
 * bunu yalnızca paylaşılan jetonla kabul ediyor (`crates/server/src/auth.rs`).
 */
function wsUrl(deviceId: string, connId: string, userId: number) {
  const base = env.get('POSTILLION_SERVER_URL', '').replace(/^http/, 'ws').replace(/\/+$/, '')
  const token = env.get('POSTILLION_SERVER_TOKEN', '')
  return (
    `${base}/device/${encodeURIComponent(deviceId)}/ws` +
    `?role=client&connId=${connId}&token=${encodeURIComponent(token)}&actAs=${userId}`
  )
}

/**
 * `method`'u hedef cihazda çalıştırır ve `ok` gövdesini döndürür.
 *
 * Röle kontrol çerçeveleri (`" relay"` — baştaki boşluk KASITLI, sunucuyla
 * bayt bayt eşleşmek zorunda) hata olarak yükseltiliyor: `host_offline`
 * cihazın kapalı olduğunu söylüyor ve bunu zaman aşımı olarak beklemek
 * kullanıcıyı 15 saniye boşuna bekletirdi.
 */
export async function call(
  deviceId: string,
  userId: number,
  method: string,
  params: unknown
): Promise<unknown> {
  const connId = randomUUID()
  const socket = new WebSocket(wsUrl(deviceId, connId, userId))
  socket.binaryType = 'arraybuffer'

  try {
    return await new Promise<unknown>((resolve, reject) => {
      const timer = setTimeout(
        () => reject(new RelayError('Cihaz zamanında cevap vermedi')),
        TIMEOUT_MS
      )
      const finish = (fn: () => void) => {
        clearTimeout(timer)
        fn()
        socket.close()
      }

      socket.onerror = () => finish(() => reject(new RelayError('Röleye bağlanılamadı')))
      socket.onclose = () => finish(() => reject(new RelayError('Bağlantı cevap gelmeden kapandı')))

      socket.onopen = () => {
        const body = JSON.stringify({ id: 1, method, params })
        socket.send(encode({ s: RPC_KIND, k: RPC_KIND }, new TextEncoder().encode(body)))
      }

      socket.onmessage = (event) => {
        let header
        let payload
        try {
          ;({ header, payload } = decode(new Uint8Array(event.data as ArrayBuffer)))
        } catch (error) {
          return finish(() => reject(new RelayError(`Çerçeve çözülemedi: ${error}`)))
        }

        if (header.k === ' relay') {
          const code = (JSON.parse(new TextDecoder().decode(payload)) as { error?: string }).error
          return finish(() =>
            reject(
              new RelayError(
                code === 'host_offline' || code === 'host_closed'
                  ? 'Cihaz çevrimdışı'
                  : `Röle hatası: ${code}`
              )
            )
          )
        }

        const frame = JSON.parse(new TextDecoder().decode(payload)) as {
          id: number
          ok?: unknown
          err?: string
        }
        if (frame.err) {
          return finish(() => reject(new RelayError(frame.err!)))
        }
        finish(() => resolve(frame.ok))
      }
    })
  } finally {
    // `close()` iki kez çağrılabilir; zaten kapalıysa bu bir no-op.
    socket.close()
  }
}

/** Sohbete bir mesaj yazar. */
export async function sendPrompt(
  deviceId: string,
  userId: number,
  chatId: string,
  prompt: string,
  cwd: string
) {
  const messageId = randomUUID()
  return call(deviceId, userId, 'QueueCommand', {
    chatId,
    command: {
      id: randomUUID(),
      // `issuedBy` panelden geldiğini söylüyor; cihazın kendi yazdıklarından
      // ayırt edilebilmesi gerekiyor.
      issuedBy: 'panel',
      issuedAt: Date.now(),
      status: 'pending',
      payload: {
        kind: 'run',
        request: { prompt, cwd, sandbox: 'workspaceWrite' },
        messageId,
      },
    },
  })
}
