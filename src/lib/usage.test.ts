import { describe, expect, it } from "vitest";

import { parseResetAt } from "./usage";

/**
 * Sıfırlanma zamanı iki ayrı kaynaktan geliyor ve biçimleri farklı: yerel
 * önbellek ISO-8601 veriyor, `/usage` komutu ise yılsız bir insan metni.
 * İkisi de doğru okunmalı, çünkü arayüz "ne zaman yine yazabilirim" sorusunu
 * bu değerle cevaplıyor.
 */
describe("parseResetAt", () => {
  it("ISO damgasını olduğu gibi okur", () => {
    // Yerel önbellekten gelen biçim; yıl ve saat dilimi tam.
    expect(parseResetAt("2026-08-17T04:49:59+00:00")).toBe(1_786_942_199_000);
    // Saniye kesiri milisaniye olarak okunuyor.
    expect(parseResetAt("2026-08-17T04:49:59.914906+00:00")).toBe(1_786_942_199_914);
    // Saat dilimi farkı uygulanmalı.
    expect(parseResetAt("2026-08-17T07:49:59+03:00")).toBe(1_786_942_199_000);
  });

  it("komut metnindeki eksik yılı geleceğe tamamlar", () => {
    // Aralık sonunda ocak tarihi gelirse bu yılın ocağı geçmişte kalırdı.
    const now = Date.parse("2026-12-28T12:00:00");
    const at = parseResetAt("Jan 3, 7am (Europe/Istanbul)", now);
    expect(at).not.toBeNull();
    expect(new Date(at as number).getFullYear()).toBe(2027);
  });

  it("tanınmayan metni reddeder", () => {
    // `Date.parse` fazla hoşgörülü; biçim açıkça doğrulanıyor.
    expect(parseResetAt("yakında")).toBeNull();
    expect(parseResetAt("2026-13-99T99:99:99+00:00")).toBeNull();
  });
});
