import { describe, expect, it } from "vitest";

import { initialState, reduce } from "./claude-stream";

/**
 * Bağlam ölçüsü ve sıkıştırma durumu.
 *
 * Maliyetin asıl sürücüsü bağlamın her turda yeniden okunması; bu ölçüm hem
 * arayüzdeki göstergeyi hem otomatik sıkıştırma eşiğini besliyor, yani yanlış
 * okunması sessizce pahalı sohbetlere yol açar.
 */
describe("bağlam ölçüsü", () => {
  const assistant = (usage: unknown) => ({
    type: "assistant",
    message: { id: "m1", content: [{ type: "text", text: "merhaba" }], usage },
  });

  it("üç token kalemini toplar", () => {
    // Yalnızca `input_tokens`'a bakmak yanıltıcı olurdu: bağlamın tamamı
    // önbellekten geldiği için o alan neredeyse hep sıfır.
    const state = reduce(
      initialState,
      assistant({
        input_tokens: 2,
        cache_creation_input_tokens: 3000,
        cache_read_input_tokens: 200_000,
        output_tokens: 50,
      }),
    );

    expect(state.contextTokens).toBe(203_002);
  });

  it("ölçüm yoksa önceki değeri korur", () => {
    const seeded = reduce(initialState, assistant({ cache_read_input_tokens: 1000 }));
    const after = reduce(seeded, assistant(undefined));

    expect(after.contextTokens).toBe(1000);
  });
});

describe("sıkıştırma durumu", () => {
  it("başlarken işaretlenir, başarıyla biterken bağlam sıfırlanır", () => {
    const busy = reduce(initialState, { type: "system", subtype: "status", status: "compacting" });
    expect(busy.compacting).toBe(true);

    const measured = { ...busy, contextTokens: 500_000 };
    const done = reduce(measured, {
      type: "system",
      subtype: "status",
      status: null,
      compact_result: "ok",
    });

    expect(done.compacting).toBe(false);
    // Eski ölçüm artık geçersiz; yenisini ilk tur getirecek. Tutulsaydı
    // otomatik sıkıştırma hemen yeniden tetiklenirdi.
    expect(done.contextTokens).toBeNull();
  });

  it("başarısız sıkıştırmada ölçüm korunur ve hata kaydedilir", () => {
    const measured = { ...initialState, compacting: true, contextTokens: 500_000 };
    const failed = reduce(measured, {
      type: "system",
      subtype: "status",
      status: null,
      compact_result: "failed",
      compact_error: "Not enough messages to compact.",
    });

    expect(failed.compacting).toBe(false);
    expect(failed.contextTokens).toBe(500_000);
    expect(failed.errors.at(-1)).toContain("Not enough messages to compact.");
  });

  it("ilgisiz durum olayları hiçbir şeyi değiştirmez", () => {
    const state = reduce(initialState, { type: "system", subtype: "status", status: "thinking" });
    expect(state).toBe(initialState);
  });
});
