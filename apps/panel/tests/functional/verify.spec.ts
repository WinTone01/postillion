import { test } from '@japa/runner'
import testUtils from '@adonisjs/core/services/test_utils'
import router from '@adonisjs/core/services/router'
import User from '#models/user'
import { verificationRequired } from '#services/mailer'

/**
 * E-posta doğrulama.
 *
 * Bağlantı imzalı, sunucuda saklanan bir jeton yok — testler de imzanın
 * kendisine bakıyor.
 */
test.group('Doğrulama', (group) => {
  group.each.setup(() => testUtils.db().withGlobalTransaction())

  async function kullanici(email = 'dogrulama@example.com') {
    return User.create({ email, password: 'cok-uzun-bir-parola' })
  }

  test('imzalı bağlantı hesabı doğruluyor', async ({ client, assert }) => {
    const user = await kullanici()
    assert.isFalse(user.isVerified, 'yeni hesap doğrulanmamış başlamalı')

    const url = router.builder().params({ id: user.id }).makeSigned('verifyEmail')
    const response = await client.get(url).redirects(0)

    response.assertStatus(302)
    await user.refresh()
    assert.isTrue(user.isVerified)
  })

  /// İmza olmadan doğrulama YAPILMAMALI: aksi hâlde kimlik numarasını tahmin
  /// eden herkes istediği hesabı doğrulayabilirdi.
  test('imzasız bağlantı hesabı doğrulamıyor', async ({ client, assert }) => {
    const user = await kullanici('imzasiz@example.com')

    const response = await client.get(`/verify/${user.id}`).redirects(0)
    response.assertStatus(302)
    response.assertHeader('location', '/login')

    await user.refresh()
    assert.isFalse(user.isVerified, 'imzasız istek hesabı doğrulamamalı')
  })

  /// Kurcalanmış imza da geçmemeli.
  test('bozulmuş imza reddediliyor', async ({ client, assert }) => {
    const user = await kullanici('bozuk@example.com')
    const url = router.builder().params({ id: user.id }).makeSigned('verifyEmail')

    const response = await client.get(`${url}x`).redirects(0)
    response.assertStatus(302)

    await user.refresh()
    assert.isFalse(user.isVerified)
  })

  /// İkinci tıklama damgayı EZMEMELİ.
  test('bağlantıya ikinci kez tıklamak tarihi değiştirmiyor', async ({ client, assert }) => {
    const user = await kullanici('ikinci@example.com')
    const url = router.builder().params({ id: user.id }).makeSigned('verifyEmail')

    await client.get(url).redirects(0)
    await user.refresh()
    const ilk = user.emailVerifiedAt!.toISO()

    await client.get(url).redirects(0)
    await user.refresh()
    assert.equal(user.emailVerifiedAt!.toISO(), ilk)
  })

  test('bekleme ekranı adresi gösteriyor', async ({ client }) => {
    const user = await kullanici('bekleme@example.com')

    const response = await client.get('/verify').loginAs(user)
    response.assertStatus(200)
    response.assertTextIncludes('bekleme@example.com')
    response.assertTextIncludes('<!DOCTYPE html>')
  })

  test('bekleme ekranı oturum istiyor', async ({ client }) => {
    const response = await client.get('/verify').redirects(0)
    response.assertStatus(302)
  })

  /// Posta gönderilemeyen bir kurulumda şart KOŞULMUYOR: koşulsaydı
  /// doğrulama bağlantısı hiç ulaşmayacağı için herkes dışarıda kalırdı.
  test('posta yapılandırılmamışken doğrulama şartı yok', async ({ assert }) => {
    assert.isFalse(verificationRequired(false))
    assert.isFalse(verificationRequired(true))
  })

  /// Posta yapılandırılmamışken kayıt panele girebilmeli.
  test('kayıt panele giriyor', async ({ client }) => {
    const response = await client
      .post('/register')
      .form({ email: 'kayit-dogrulama@example.com', password: 'cok-uzun-bir-parola' })
      .withCsrfToken()
      .redirects(0)

    response.assertStatus(302)
    response.assertHeader('location', '/app')
  })
})
