import { BaseSchema } from '@adonisjs/lucid/schema'

/**
 * Son doğrulama postasının zamanı.
 *
 * Oturumda değil veritabanında: bekleme süresi KULLANICIYA bağlı olmalı.
 * Oturumda tutulsaydı çerezleri silmek ya da başka bir tarayıcı açmak onu
 * sıfırlardı ve buton yine bir spam aracı olurdu.
 */
export default class extends BaseSchema {
  protected tableName = 'users'

  async up() {
    this.schema.alterTable(this.tableName, (table) => {
      table.timestamp('verification_sent_at').nullable()
    })
  }

  async down() {
    this.schema.alterTable(this.tableName, (table) => {
      table.dropColumn('verification_sent_at')
    })
  }
}
