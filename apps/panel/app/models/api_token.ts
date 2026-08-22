import { DateTime } from 'luxon'
import { BaseModel, column } from '@adonisjs/lucid/orm'
import { createHash, randomBytes } from 'node:crypto'

/**
 * Bir cihazın sunucuya bağlanmak için kullandığı jeton.
 *
 * Tablonun ŞEMASI bu uygulamada tanımlı değil: `postillion-server` açılışta
 * kuruyor (`crates/server/src/identity_db.rs`). İki yerde tanımlamak, ikisinin
 * zamanla ayrılması ve hangisinin önce koştuğuna bağlı bir veritabanı
 * demekti. Panel yalnızca okuyup yazıyor.
 */
export default class ApiToken extends BaseModel {
  static table = 'api_tokens'

  @column({ isPrimary: true })
  declare id: number

  @column()
  declare userId: number

  @column()
  declare name: string

  /**
   * Jetonun SHA-256 özeti. Ham değer hiçbir zaman saklanmıyor — üretildiği an
   * bir kez gösterilip unutuluyor.
   */
  @column({ serializeAs: null })
  declare tokenHash: string

  @column.dateTime({ autoCreate: true })
  declare createdAt: DateTime

  @column.dateTime()
  declare lastUsedAt: DateTime | null

  /**
   * Yeni bir jeton üretir ve ham değeriyle birlikte döndürür.
   *
   * Onaltılık, base64 DEĞİL: bu jeton istemci tarafında kullanıcı kimliği
   * olarak da kullanılıp veri dizininde bir yol parçasına dönüşüyor ve
   * base64'teki `/` o yolu bölerdi. `@` de kimliği kırpıyor.
   *
   * 32 bayt: kaba kuvvetle tahmin edilemeyecek kadar geniş, elle kopyalanacak
   * kadar kısa.
   */
  static async mint(userId: number, name: string) {
    const raw = randomBytes(32).toString('hex')
    const token = await ApiToken.create({
      userId,
      name,
      tokenHash: ApiToken.hash(raw),
    })
    return { token, raw }
  }

  /** Sunucudaki `hash_token` ile AYNI olmak zorunda (`crates/server/src/auth.rs`). */
  static hash(raw: string) {
    return createHash('sha256').update(raw).digest('hex')
  }
}
