import { test } from '@japa/runner'
import testUtils from '@adonisjs/core/services/test_utils'
import db from '@adonisjs/lucid/services/db'
import ApiToken from '#models/api_token'
import User from '#models/user'

/**
 * Cihaz jetonları.
 *
 * Tablonun şeması `postillion-server` tarafından kuruluyor; testler onu
 * kendileri oluşturuyor çünkü panelin göçleri arasında yok (bilerek — iki
 * yerde tanımlamak ikisinin ayrılması demekti).
 */
test.group('Jetonlar', (group) => {
  group.setup(async () => {
    await db.rawQuery(`
      create table if not exists api_tokens (
        id           bigserial   primary key,
        user_id      bigint      not null,
        name         text        not null,
        token_hash   text        not null unique,
        created_at   timestamptz not null default now(),
        last_used_at timestamptz
      )
    `)
  })
  group.each.setup(() => testUtils.db().withGlobalTransaction())

  async function newUser() {
    return User.create({
      email: `u${Date.now()}${Math.random()}@example.com`,
      password: 'cok-uzun-bir-parola',
    })
  }

  test('jeton üretiliyor ve ham değeri BİR KEZ gösteriliyor', async ({ client, assert }) => {
    const user = await newUser()

    const response = await client
      .post('/app/tokens')
      .form({ name: 'Dizüstü' })
      .withCsrfToken()
      .loginAs(user)
      .redirects(0)
    response.assertStatus(302)

    const page = await client.get('/app/tokens').loginAs(user)
    page.assertStatus(200)

    const stored = await ApiToken.query().where('user_id', user.id).firstOrFail()
    assert.equal(stored.name, 'Dizüstü')
    // Ham jeton ASLA saklanmıyor.
    assert.lengthOf(stored.tokenHash, 64, 'sha-256 onaltılık')
  })

  /// Özet sunucununkiyle AYNI olmak zorunda; olmazsa jeton hiçbir zaman
  /// doğrulanmaz ve hata yalnızca çalışan sistemde görünür.
  test('özet sunucunun beklediği biçimde', async ({ assert }) => {
    // `sha256("postillion")` — sunucudaki `hash_token` ile aynı olmalı.
    assert.equal(
      ApiToken.hash('postillion'),
      '0a32066d31ecf44c0a22ccd8a7c3f9422228893b38c52ac3587fef056d228495'
    )
  })

  test('jeton onaltılık ve yol bozan karakter içermiyor', async ({ assert }) => {
    const user = await User.create({
      email: `hex${Date.now()}@example.com`,
      password: 'cok-uzun-bir-parola',
    })
    const { raw } = await ApiToken.mint(user.id, 'Test')

    // `/` profil yolunu böler, `@` kimliği kırpar — istemci tarafında bu
    // jeton kullanıcı kimliği olarak da kullanılıyor.
    assert.match(raw, /^[0-9a-f]{64}$/)
  })

  test('başkasının jetonu iptal edilemiyor', async ({ client, assert }) => {
    const sahip = await User.create({
      email: `sahip${Date.now()}@example.com`,
      password: 'cok-uzun-bir-parola',
    })
    const baskasi = await User.create({
      email: `baska${Date.now()}@example.com`,
      password: 'cok-uzun-bir-parola',
    })
    const { token } = await ApiToken.mint(sahip.id, 'Dizüstü')

    await client
      .delete(`/app/tokens/${token.id}`)
      .withCsrfToken()
      .loginAs(baskasi)
      .redirects(0)

    assert.isNotNull(
      await ApiToken.find(token.id),
      'başkasının jetonu silinmemeli'
    )
  })

  test('kendi jetonu iptal edilebiliyor', async ({ client, assert }) => {
    const user = await User.create({
      email: `iptal${Date.now()}@example.com`,
      password: 'cok-uzun-bir-parola',
    })
    const { token } = await ApiToken.mint(user.id, 'Dizüstü')

    await client.delete(`/app/tokens/${token.id}`).withCsrfToken().loginAs(user).redirects(0)
    assert.isNull(await ApiToken.find(token.id))
  })

  test('jeton sayfası kimliksiz açılmıyor', async ({ client }) => {
    const response = await client.get('/app/tokens').redirects(0)
    response.assertStatus(302)
  })
})
