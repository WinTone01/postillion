import { test } from '@japa/runner'
import testUtils from '@adonisjs/core/services/test_utils'
import User from '#models/user'

/**
 * Kayıt ve giriş akışı.
 *
 * Her test kendi işleminde koşuyor ve sonunda geri alınıyor: aksi hâlde bir
 * testin oluşturduğu kullanıcı sonrakinin "bu e-posta zaten kayıtlı"
 * yoluna düşmesine yol açardı.
 */
test.group('Kimlik', (group) => {
  group.each.setup(() => testUtils.db().withGlobalTransaction())

  test('kayıt formu açılıyor', async ({ client }) => {
    const response = await client.get('/register')
    response.assertStatus(200)
    response.assertTextIncludes('Hesap oluştur')
  })

  test('kayıt hesabı oluşturup oturum açıyor', async ({ client, assert }) => {
    const response = await client
      .post('/register')
      .form({ email: 'deneme@example.com', password: 'cok-uzun-bir-parola' })
      .withCsrfToken()
      .redirects(0)

    response.assertStatus(302)
    const user = await User.findBy('email', 'deneme@example.com')
    assert.isNotNull(user, 'kullanıcı oluşturulmalı')
    // Parola ASLA düz saklanmamalı.
    assert.notEqual(user!.password, 'cok-uzun-bir-parola')
  })

  test('kısa parola reddediliyor', async ({ client, assert }) => {
    const response = await client
      .post('/register')
      .form({ email: 'kisa@example.com', password: 'kisa' })
      .withCsrfToken()
      .redirects(0)

    response.assertStatus(302)
    assert.isNull(await User.findBy('email', 'kisa@example.com'))
  })

  test('aynı e-posta ikinci kez kayıt olamıyor', async ({ client, assert }) => {
    await User.create({ email: 'var@example.com', password: 'cok-uzun-bir-parola' })

    await client
      .post('/register')
      .form({ email: 'var@example.com', password: 'baska-uzun-parola' })
      .withCsrfToken()
      .redirects(0)

    const all = await User.query().where('email', 'var@example.com')
    assert.lengthOf(all, 1, 'ikinci kayıt oluşmamalı')
  })

  test('doğru parolayla giriş yapılıyor', async ({ client }) => {
    await User.create({ email: 'giris@example.com', password: 'cok-uzun-bir-parola' })

    const response = await client
      .post('/login')
      .form({ email: 'giris@example.com', password: 'cok-uzun-bir-parola' })
      .withCsrfToken()
      .redirects(0)

    response.assertStatus(302)
    response.assertHeader('location', '/')
  })

  test('yanlış parola girişi reddediyor', async ({ client }) => {
    await User.create({ email: 'yanlis@example.com', password: 'cok-uzun-bir-parola' })

    const response = await client
      .post('/login')
      .form({ email: 'yanlis@example.com', password: 'yanlis-olan-parola' })
      .withCsrfToken()
      .redirects(0)

    // Geri yönlendiriliyor ve OTURUM AÇILMIYOR. Hedefi kontrol etmiyoruz:
    // `redirect().back()` Referer olmadığında köke düşüyor ve bu testin
    // ölçtüğü şey değil.
    response.assertStatus(302)
    const after = await client.get('/').redirects(0)
    after.assertStatus(302)
  })

  test('CSRF jetonu olmadan kayıt reddediliyor', async ({ client, assert }) => {
    // Shield'in açık olduğunu KANITLIYOR: jetonsuz bir POST kabul edilirse
    // formların hiçbiri korunmuyor demektir.
    await client
      .post('/register')
      .form({ email: 'csrf@example.com', password: 'cok-uzun-bir-parola' })
      .redirects(0)

    assert.isNull(await User.findBy('email', 'csrf@example.com'))
  })

  test('panel kimliksiz açılmıyor', async ({ client }) => {
    const response = await client.get('/').redirects(0)
    response.assertStatus(302)
  })
})
