import { test } from '@japa/runner'
import testUtils from '@adonisjs/core/services/test_utils'
import router from '@adonisjs/core/services/router'
import User from '#models/user'
import { DateTime } from 'luxon'
import {
  RESEND_COOLDOWN_SECONDS,
  appBaseUrl,
  mailConfigured,
  resendCooldownRemaining,
  verificationRequired,
  verificationUrl,
} from '#services/mailer'

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

/**
 * Bağlantının MUTLAK olması.
 *
 * Bunu yakalayan hiçbir test yoktu ve üretimde `APP_URL` boş kaldı: posta
 * `/verify/1?signature=…` ile gitti, tarayıcı `verify`'ı sunucu adı sandı ve
 * kimse hesabını doğrulayamadı. Hiçbir yerde hata görünmedi — bağlantı
 * geçerli bir imza taşıyordu, yalnızca hiçbir yere gitmiyordu.
 */
test.group('Doğrulama bağlantısının kökü', () => {
  test('APP_URL boşken çağıranın verdiği kök kullanılıyor', async ({ assert }) => {
    const user = { id: 7 } as User
    const url = verificationUrl(user, 'https://panel.example.com')
    assert.isTrue(url.startsWith('https://panel.example.com/verify/7'), url)
  })

  test('şemasız APP_URL tamamlanıyor', async ({ assert }) => {
    // `postillion.net` yazmak kolay ve şemasız bir kök yine göreli bir
    // bağlantı üretirdi.
    assert.equal(appBaseUrl('postillion.net'), 'https://postillion.net')
    assert.equal(appBaseUrl('https://postillion.net/'), 'https://postillion.net')
  })

  test('kök hiç yoksa posta yapılandırılmış SAYILMIYOR', async ({ assert }) => {
    // Açılmayan bir bağlantı içeren posta göndermek, hiç göndermemekten
    // kötü: kullanıcı gelen kutusuna bakıp bekler.
    assert.equal(appBaseUrl(), '')
    assert.isFalse(mailConfigured())
  })
})

/**
 * Yeniden gönderim bir SPAM ARACI olabilir: saldırgan başkasının adresiyle
 * kaydolup butona basmaya devam ederse o kutuyu doldurur.
 */
test.group('Yeniden gönderim beklemesi', () => {
  test('taze hesapta bekleme yok', async ({ assert }) => {
    const user = { verificationSentAt: null } as User
    assert.equal(resendCooldownRemaining(user), 0)
  })

  test('az önce gönderilmişse bekleme sürüyor', async ({ assert }) => {
    const now = DateTime.now()
    const user = { verificationSentAt: now.minus({ seconds: 5 }) } as User
    assert.equal(resendCooldownRemaining(user, now), RESEND_COOLDOWN_SECONDS - 5)
  })

  test('süre dolunca serbest', async ({ assert }) => {
    const now = DateTime.now()
    const user = {
      verificationSentAt: now.minus({ seconds: RESEND_COOLDOWN_SECONDS + 1 }),
    } as User
    assert.equal(resendCooldownRemaining(user, now), 0)
  })
})
