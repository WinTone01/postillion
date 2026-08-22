import { test } from '@japa/runner'
import testUtils from '@adonisjs/core/services/test_utils'
import { CAPTCHA_TOKEN_FIELD } from '#controllers/auth_controller'

/**
 * Widget'ın gönderdiği alan adı ile paketin okuduğu ad AYNI olmalı.
 *
 * Tutmadıklarında hiçbir şey derlemede ya da başlangıçta patlamıyor: sayfa
 * açılıyor, captcha çözülüyor, sonra POST 400 "Request is missing the token
 * param" ile düşüyor. Üretimde tam olarak bu oldu ve testlerin hepsi geçti,
 * çünkü site anahtarı yokken doğrulama tümüyle atlanıyor.
 *
 * Bu yüzden karşılaştırma sabite değil paketin KENDİ değerine yapılıyor:
 * `tokenParamName` bir gün değişirse test onu da yakalar.
 */
test.group('Captcha alan adı', () => {
  test('widget jetonu paketin aradığı adla gönderiyor', async ({ assert }) => {
    const ctx = await testUtils.createHttpContext()
    const expected = (ctx.captcha.use('turnstile') as unknown as { tokenParamName: string })
      .tokenParamName

    assert.equal(CAPTCHA_TOKEN_FIELD, expected, 'denetleyicinin ön kontrolü aynı adı okumalı')

    for (const page of ['pages/auth/login', 'pages/auth/register']) {
      // Site anahtarı verilerek işleniyor: anahtar boşken widget hiç
      // basılmıyor ve test hiçbir şey ölçmemiş olurdu.
      //
      // `csrfField` ve `flashMessages` normalde ara katmanların paylaştığı
      // değerler; bu bağlam onları çalıştırmadan kuruluyor, o yüzden boş
      // karşılıkları veriliyor. Ölçülen şey captcha alanı, düzenin geri
      // kalanı başka testlerin işi.
      const html = await ctx.view.render(page, {
        turnstileSiteKey: 'test-site-key',
        csrfField: () => '',
        flashMessages: { has: () => false, get: () => undefined },
      })
      assert.include(
        html,
        `data-response-field-name="${expected}"`,
        `${page} jetonu "${expected}" adıyla göndermeli`
      )
    }
  })
})
