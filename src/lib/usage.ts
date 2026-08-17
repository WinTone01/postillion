import type { Usage, UsageWindow } from "@/api";

/**
 * `/usage` çıktısındaki sıfırlanma zamanını epoch'a çevirir.
 *
 * Gelen biçim: `Aug 14, 3:50pm (Europe/Istanbul)` — yıl yok ve parantez
 * içindeki saat dilimi zaten kullanıcının yereli, yani metin yerel saat olarak
 * okunmalı. `Date.parse` tam da öyle yapıyor.
 *
 * Yıl eksik olduğu için hem bu yıl hem gelecek yıl deneniyor ve **gelecekteki
 * en yakın** olan seçiliyor: aralık sonunda ocak tarihi gelirse bu yılın ocağı
 * geçmişte kalır ve yanlış olurdu.
 */
export function parseResetAt(resets: string, now = Date.now()): number | null {
  // Yerel önbellekten gelen değer ISO-8601: tam ve kesin, tahmin gerekmiyor.
  // Komuttan gelen metinde ise yıl yok, aşağıdaki yol onun için.
  if (/^\d{4}-\d{2}-\d{2}T/.test(resets)) {
    const parsed = Date.parse(resets);
    return Number.isNaN(parsed) ? null : parsed;
  }

  // Saat dilimi adını at; `Date.parse` "(Europe/Istanbul)" ile baş edemiyor.
  const cleaned = resets.replace(/\s*\([^)]*\)\s*$/, "").trim();

  const [datePart = "", timePart = ""] = cleaned.split(",").map((part) => part.trim());

  // Biçim açıkça doğrulanıyor: `Date.parse` çok hoşgörülü ve "yakında 2026"
  // gibi bir metinden bile bir tarih üretiyor.
  if (!/^[A-Za-z]{3,9}\s+\d{1,2}$/.test(datePart)) return null;

  // "7am" ve "3:50pm" — dakika opsiyonel, ayraç yok. İkisini de motorların
  // kabul ettiği "7:00 am" biçimine getiriyoruz.
  const time = timePart.match(/^(\d{1,2})(?::(\d{2}))?\s*(am|pm)$/i);
  if (!time) return null;
  const normalised = `${time[1]}:${time[2] ?? "00"} ${time[3].toLowerCase()}`;

  const thisYear = new Date(now).getFullYear();
  let best: number | null = null;

  // Yıl eksik: geçmişte kalanı atlayıp en yakın geleceği alıyoruz.
  for (const year of [thisYear, thisYear + 1]) {
    const parsed = Date.parse(`${datePart} ${year} ${normalised}`);
    if (Number.isNaN(parsed) || parsed < now) continue;
    if (best === null || parsed < best) best = parsed;
  }

  return best;
}

/**
 * Gösterilecek sıfırlanma penceresi.
 *
 * Normalde oturum (en dar) penceresi ilgilendiriyor. Ama haftalık pay bittiyse
 * oturumun sıfırlanması bir şey ifade etmiyor — o hafta boyunca yazamazsınız —
 * o yüzden haftalık pencere öne geçiyor.
 */
export function resetWindow(usage: Usage): UsageWindow | null {
  const withReset = usage.windows.filter((w) => w.resets);
  if (withReset.length === 0) return null;

  const weekly = withReset.find((w) => w.label.startsWith("week"));
  if (weekly && weekly.percent >= 100) return weekly;

  return withReset.find((w) => w.label === "session") ?? weekly ?? withReset[0];
}
