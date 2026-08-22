import { DateTime } from 'luxon'
import hash from '@adonisjs/core/services/hash'
import { compose } from '@adonisjs/core/helpers'
import { BaseModel, column } from '@adonisjs/lucid/orm'
import { withAuthFinder } from '@adonisjs/auth/mixins/lucid'

const AuthFinder = withAuthFinder(() => hash.use('scrypt'), {
  uids: ['email'],
  passwordColumnName: 'password',
})

export default class User extends compose(BaseModel, AuthFinder) {
  @column({ isPrimary: true })
  declare id: number

  @column()
  declare fullName: string | null

  @column()
  declare email: string

  @column({ serializeAs: null })
  declare password: string

  /** `null` = e-posta doğrulanmadı. */
  @column.dateTime()
  declare emailVerifiedAt: DateTime | null

  /** Son doğrulama postasının zamanı — yeniden gönderim bekleme süresi. */
  @column.dateTime()
  declare verificationSentAt: DateTime | null

  @column.dateTime({ autoCreate: true })
  declare createdAt: DateTime

  @column.dateTime({ autoCreate: true, autoUpdate: true })
  declare updatedAt: DateTime | null

  /**
   * `!= null` GEVŞEK karşılaştırma ve öyle olmak zorunda: sütunu hiç
   * atanmamış taze bir model `undefined` taşıyor, veritabanından gelen ise
   * `null`. Katı `!== null` ilkini "doğrulanmış" sayıyordu — yani yeni
   * kaydolan herkes doğrulamayı atlıyordu.
   */
  get isVerified() {
    return this.emailVerifiedAt != null
  }
}
