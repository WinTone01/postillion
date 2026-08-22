import env from '#start/env'
import { defineConfig } from '@adonisjs/lucid'

const dbConfig = defineConfig({
  connection: 'postgres',
  connections: {
    postgres: {
      client: 'pg',
      // Bağlantı dizesi varsa o kazanıyor: dağıtımda panel eşitleme
      // sunucusuyla AYNI `DATABASE_URL`'i alıyor ve beş ayrı değişkeni
      // doğru girmek zorunda kalmıyor. Parçalı biçim yerel geliştirmede.
      connection: env.get('DATABASE_URL')
        ? env.get('DATABASE_URL')!
        : {
            host: env.get('DB_HOST'),
            port: env.get('DB_PORT'),
            user: env.get('DB_USER'),
            password: env.get('DB_PASSWORD'),
            database: env.get('DB_DATABASE'),
          },
      migrations: {
        naturalSort: true,
        paths: ['database/migrations'],
      },
    },
  },
})

export default dbConfig