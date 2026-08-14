import { writeText } from "@tauri-apps/plugin-clipboard-manager";

import { log } from "@/lib/log";

/**
 * `navigator.clipboard.writeText`'i Tauri panosuna bağlar.
 *
 * Streamdown'ın kod bloğu kopyalama butonu — ve genel olarak kopyalayan her
 * bileşen — tarayıcı API'sini çağırıyor. WebKitGTK bu API için güvenli bağlam
 * (`https` ya da `localhost`) şart koşuyor; Tauri sayfayı kendi özel
 * protokolünden sunduğu için çağrı ya hiç yok ya da sessizce reddediliyor.
 * Butonlar tıklanıyor ama hiçbir şey kopyalanmıyordu.
 *
 * Bileşenleri tek tek yamamak yerine API'nin kendisi yerine konuyor: satıcı
 * kodu değişmeden çalışıyor.
 */
export function installClipboardBridge() {
  const native = navigator.clipboard?.writeText?.bind(navigator.clipboard);

  const write = async (text: string) => {
    try {
      await writeText(text);
      return;
    } catch (e) {
      log("warn", "pano köprüsü başarısız, tarayıcıya düşülüyor:", e);
    }
    // Köprü yoksa (ör. tarayıcıda geliştirme) özgün davranış korunsun.
    if (native) return native(text);
    throw new Error("pano kullanılamıyor");
  };

  // Kalan metotlar özgün nesneye bağlanarak taşınıyor. Nesneyi yaymak
  // (`{ ...navigator.clipboard }`) hiçbirini kopyalamaz — hepsi prototipte;
  // `this` bağlanmadan çağırmak da "illegal invocation" verir.
  const value: Record<string, unknown> = { writeText: write };
  const source = navigator.clipboard as unknown as Record<string, unknown> | undefined;
  if (source) {
    for (const name of ["read", "readText", "write"]) {
      const method = source[name];
      if (typeof method === "function") value[name] = method.bind(navigator.clipboard);
    }
  }

  try {
    Object.defineProperty(navigator, "clipboard", { configurable: true, value });
  } catch (e) {
    log("warn", "pano köprüsü kurulamadı:", e);
  }
}
