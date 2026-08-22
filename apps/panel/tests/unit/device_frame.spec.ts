import { test } from '@japa/runner'
import { decode, encode } from '#services/device_frame'

/**
 * Çerçeve codec'i.
 *
 * Sunucunun `encode_device_frame` / `decode_device_frame` biçimiyle bayt bayt
 * aynı olmak zorunda. Kodlama hatası canlı bir bağlantıda "hiçbir şey
 * olmuyor" olarak görünürdü, o yüzden burada ağsız sınanıyor.
 */
test.group('Cihaz çerçevesi', () => {
  test('gidiş dönüş', ({ assert }) => {
    const payload = new TextEncoder().encode('{"id":1}')
    const { header, payload: back } = decode(encode({ s: 'rpc', k: 'rpc' }, payload))
    assert.deepEqual(header, { s: 'rpc', k: 'rpc' })
    assert.deepEqual(Array.from(back), Array.from(payload))
  })

  test('uzun başlık iki bayta taşıyor', ({ assert }) => {
    // 127 baytı aşan başlık uleb128'de iki bayta taşıyor; tek baytlık bir
    // uzunluk yazsaydık uzun başlıklarda sessizce bozulurdu.
    const long = 'x'.repeat(300)
    const frame = encode({ s: long, k: 'rpc' }, new Uint8Array())
    assert.isAbove(frame[0], 127, 'ilk baytın devam biti açık olmalı')
    assert.equal(decode(frame).header.s, long)
  })

  test('boş yük taşınabiliyor', ({ assert }) => {
    const { payload } = decode(encode({ s: 'a', k: 'b' }, new Uint8Array()))
    assert.lengthOf(payload, 0)
  })

  test('kesik çerçeve hata veriyor', ({ assert }) => {
    const frame = encode({ s: 'rpc', k: 'rpc' }, new TextEncoder().encode('yük'))
    assert.throws(() => decode(frame.subarray(0, 3)))
  })

  test('yönlendirme anahtarları taşınıyor', ({ assert }) => {
    const { header } = decode(encode({ s: 'rpc', k: 'rpc', to: 'c1' }, new Uint8Array()))
    assert.equal(header.to, 'c1')
  })
})
