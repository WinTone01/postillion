import { BaseSchema } from '@adonisjs/lucid/schema'

/**
 * E-posta doğrulama damgası.
 *
 * Ayrı bir jeton tablosu YOK: doğrulama bağlantısı imzalı URL, yani sunucu
 * tarafında saklanacak bir sır bulunmuyor ve süresi imzanın içinde. Tablo
 * tutmak, temizlenmeyi bekleyen bir sıra daha demekti.
 */
export default class extends BaseSchema {
  protected tableName = 'users'

  async up() {
    this.schema.alterTable(this.tableName, (table) => {
      // `null` = doğrulanmamış. Mevcut kullanıcılar da öyle başlıyor;
      // varsayılan olarak doğrulanmış saymak, kuralı geçmişe dönük
      // atlatmak olurdu.
      table.timestamp('email_verified_at').nullable()
    })
  }

  async down() {
    this.schema.alterTable(this.tableName, (table) => {
      table.dropColumn('email_verified_at')
    })
  }
}
