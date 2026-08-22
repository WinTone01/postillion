import vine from '@vinejs/vine'

/**
 * Kayıt girdisi.
 *
 * Parola alt sınırı 12: bu hesap sunucudaki bütün sohbetlere açılan kapı ve
 * uçtan uca şifreleme yok, yani parola tek savunma katmanı.
 */
export const registerValidator = vine.compile(
  vine.object({
    email: vine.string().trim().email().normalizeEmail(),
    password: vine.string().minLength(12),
  })
)

export const loginValidator = vine.compile(
  vine.object({
    email: vine.string().trim().email(),
    // Girişte uzunluk KONTROL EDİLMİYOR: eski bir parola kuralı değiştiğinde
    // sahibi hesabına giremez hale gelirdi.
    password: vine.string(),
  })
)
