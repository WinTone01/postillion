/**
 * İki dilli arayüz: Türkçe ve İngilizce.
 *
 * Anahtar olarak Türkçe kaynak metnin kendisi kullanılıyor (gettext'in yaptığı
 * gibi). Uydurma anahtarlar (`session.new` gibi) yüzlerce satırı okunmaz hale
 * getirirdi ve bu kod tabanı Türkçe yazılmış; kaynak metin hem anahtar hem de
 * varsayılan çeviri olunca dosyalar okunabilir kalıyor.
 *
 * Bedeli: Türkçe metni değiştirmek çeviriyi sessizce düşürür. `npm run
 * check:i18n` tam da bunu yakalıyor — eksik ya da artık kullanılmayan
 * anahtarları listeliyor.
 */

import { EN } from "@/lib/i18n-en";

export type Lang = "tr" | "en";

const STORAGE_KEY = "postillion.lang";

/**
 * Sistem dili.
 *
 * WebKitGTK `navigator.language`'i ortamın yerel ayarından (`LANG`/`LC_ALL`)
 * türetiyor. Yalnızca **birincil** dile bakılıyor: ikinci sıradaki bir dil
 * kullanıcının tercihi değil, yedeği.
 */
export function detectLanguage(tag: string | undefined): Lang {
  const primary = (tag || "en").toLowerCase().split("-")[0];
  // Türkçe dışındaki her dil İngilizce'ye düşüyor.
  return primary === "tr" ? "tr" : "en";
}

function stored(): Lang | null {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    return raw === "tr" || raw === "en" ? raw : null;
  } catch {
    return null;
  }
}

/**
 * Etkin dil.
 *
 * Bir kez hesaplanıp sabit kalıyor: dil ortasında değişen bir arayüz, her
 * metnin React durumuna bağlanmasını gerektirirdi. Ayarlardan değiştirmek
 * pencereyi yeniden yüklüyor.
 */
export const lang: Lang = stored() ?? detectLanguage(navigator.language);

/** Ayarlardaki seçim; `null` "sistemi izle" demek. */
export function languageOverride(): Lang | null {
  return stored();
}

export function setLanguageOverride(next: Lang | null) {
  try {
    if (next === null) localStorage.removeItem(STORAGE_KEY);
    else localStorage.setItem(STORAGE_KEY, next);
  } catch {
    // Depolama yoksa seçim kalıcı olmaz; arayüz yine de çalışır.
  }
}

/** Eksik çeviriler bir kez uyarılsın; her render'da değil. */
const warned = new Set<string>();

/**
 * Metni etkin dile çevirir.
 *
 * `vars` verilirse `{ad}` yer tutucuları doldurulur.
 */
export function t(source: string, vars?: Record<string, string | number>): string {
  let text = source;

  if (lang === "en") {
    const translated = EN[source];
    if (translated === undefined) {
      // Anahtar başına bir kez; eksik çeviri arayüzü bozmuyor ama sessizce
      // Türkçe kalması da fark edilmemeli.
      if (!warned.has(source)) {
        warned.add(source);
        console.warn(`[i18n] çeviri eksik: ${JSON.stringify(source)}`);
      }
    } else {
      text = translated;
    }
  }

  if (!vars) return text;
  return text.replace(/\{(\w+)\}/g, (whole, name: string) =>
    name in vars ? String(vars[name]) : whole,
  );
}

/**
 * Gelecekteki bir ana kalan süre.
 *
 * `formatRelative`'in aynası: o geçmişe bakıyor, bu ileriye. Kullanım
 * göstergesinde payın ne zaman geri geleceğini yazıyor.
 */
export function formatUntil(ms: number): string {
  const min = Math.round((ms - Date.now()) / 60000);
  if (min <= 1) return t("birazdan");
  if (min < 60) return t("{n} dk sonra", { n: min });

  const hour = Math.round(min / 60);
  if (hour < 24) return t("{n} sa sonra", { n: hour });

  return t("{n} gün sonra", { n: Math.round(hour / 24) });
}

/** Yüzde: Türkçe işareti önce, İngilizce sonra yazıyor. */
export function percent(value: number): string {
  return lang === "tr" ? `%${value}` : `${value}%`;
}

/**
 * Göreli zaman.
 *
 * `Intl.RelativeTimeFormat` yerine elle: çıktı "3 sa önce" gibi kısa olmalı ve
 * Intl'in Türkçe çıktısı ("3 saat önce") sekme etiketlerine sığmıyordu.
 */
export function formatRelative(ms: number): string {
  const diff = Date.now() - ms;
  const min = Math.floor(diff / 60000);
  if (min < 1) return t("az önce");
  if (min < 60) return t("{n} dk önce", { n: min });

  const hour = Math.floor(min / 60);
  if (hour < 24) return t("{n} sa önce", { n: hour });

  const day = Math.floor(hour / 24);
  if (day < 30) return t("{n} gün önce", { n: day });

  return new Date(ms).toLocaleDateString(lang === "tr" ? "tr-TR" : "en-US");
}
