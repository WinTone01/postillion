/*
|--------------------------------------------------------------------------
| Environment variables service
|--------------------------------------------------------------------------
|
| The `Env.create` method creates an instance of the Env service. The
| service validates the environment variables and also cast values
| to JavaScript data types.
|
*/

import { Env } from '@adonisjs/core/env'

export default await Env.create(new URL('../', import.meta.url), {
  NODE_ENV: Env.schema.enum(['development', 'production', 'test'] as const),
  PORT: Env.schema.number(),
  APP_KEY: Env.schema.string(),
  HOST: Env.schema.string({ format: 'host' }),
  LOG_LEVEL: Env.schema.string(),

  /*
  |----------------------------------------------------------
  | Variables for configuring session package
  |----------------------------------------------------------
  */
  SESSION_DRIVER: Env.schema.enum(['cookie', 'memory'] as const),

  /*
  |----------------------------------------------------------
  | Variables for configuring database connection
  |----------------------------------------------------------
  */
  /*
  |----------------------------------------------------------
  | Veritabanı
  |----------------------------------------------------------
  |
  | `DATABASE_URL` tek başına yeterli ve dağıtımda kullanılan yol bu:
  | eşitleme sunucusuyla AYNI dizeyi alıyor, beş ayrı değişken yerine bir
  | tane. Parçalı `DB_*` değişkenleri yerel geliştirme için duruyor ve bu
  | yüzden hepsi isteğe bağlı — biri eksikse bağlantı dizesi kullanılıyor.
  */
  DATABASE_URL: Env.schema.string.optional(),
  DB_HOST: Env.schema.string.optional({ format: 'host' }),
  DB_PORT: Env.schema.number.optional(),
  DB_USER: Env.schema.string.optional(),
  DB_PASSWORD: Env.schema.string.optional(),
  DB_DATABASE: Env.schema.string.optional(),

  /*
  |----------------------------------------------------------
  | Eşitleme sunucusu — canlılık ve transkript buradan geliyor
  |----------------------------------------------------------
  */
  POSTILLION_SERVER_URL: Env.schema.string.optional(),
  POSTILLION_SERVER_TOKEN: Env.schema.string.optional(),

  TURNSTILE_SITE_KEY: Env.schema.string.optional(),
  TURNSTILE_SECRET: Env.schema.string.optional(),
  RECAPTCHA_SITE_KEY: Env.schema.string.optional(),
  RECAPTCHA_SECRET: Env.schema.string.optional()
})
