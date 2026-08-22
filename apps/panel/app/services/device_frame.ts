/**
 * Cihaz rölesinin çerçeve codec'i.
 *
 * Biçim `crates/rpc/src/device_room.rs` ile BAYT BAYT aynı olmak zorunda:
 *
 *     uleb128(başlık_uzunluğu) ‖ UTF-8 JSON başlık ‖ yük
 *
 * Ayrı bir dosyada çünkü ağ olmadan sınanabiliyor — kodlama hatası, canlı bir
 * bağlantıda "hiçbir şey olmuyor" olarak görünürdü.
 */

export interface FrameHeader {
  /** Akış kimliği. */
  s: string
  /** Akış türü — röle için opak. */
  k: string
  /** host → istemci hedefi. */
  to?: string
  /** istemci → host kaynağı; röle damgalıyor. */
  from?: string
}

/** uleb128: 7 bit veri, en yüksek bit "devam ediyor". */
function uleb128(value: number): number[] {
  const out: number[] = []
  let n = value
  for (;;) {
    let byte = n & 0x7f
    n >>>= 7
    if (n !== 0) {
      byte |= 0x80
      out.push(byte)
    } else {
      out.push(byte)
      return out
    }
  }
}

export function encode(header: FrameHeader, payload: Uint8Array): Uint8Array {
  const json = new TextEncoder().encode(JSON.stringify(header))
  const prefix = uleb128(json.length)
  const out = new Uint8Array(prefix.length + json.length + payload.length)
  out.set(prefix, 0)
  out.set(json, prefix.length)
  out.set(payload, prefix.length + json.length)
  return out
}

export function decode(bytes: Uint8Array): { header: FrameHeader; payload: Uint8Array } {
  let offset = 0
  let length = 0
  let shift = 0
  for (;;) {
    if (offset >= bytes.length) {
      throw new Error('device frame: uleb128 kesik')
    }
    const byte = bytes[offset++]
    if (shift >= 32) {
      throw new Error('device frame: uleb128 taştı')
    }
    length |= (byte & 0x7f) << shift
    if ((byte & 0x80) === 0) {
      break
    }
    shift += 7
  }
  if (offset + length > bytes.length) {
    throw new Error('device frame: başlık kesik')
  }
  const header = JSON.parse(
    new TextDecoder().decode(bytes.subarray(offset, offset + length))
  ) as FrameHeader
  return { header, payload: bytes.subarray(offset + length) }
}
