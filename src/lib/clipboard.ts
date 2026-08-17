import { writeText } from "@tauri-apps/plugin-clipboard-manager";

import { api } from "@/api";
import { attachmentToFile, isSupportedImage } from "@/lib/images";
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

// ------------------------------------------------------------- yapıştırma

/**
 * Bir yapıştırma olayındaki görüntüleri çıkarır.
 *
 * İki kaynağa birden bakıyor. `items` yalnızca `kind === "file"` filtresiyle
 * okunuyordu; WebKitGTK panodaki görüntüyü her zaman öyle işaretlemiyor, o
 * yüzden ölçüt tür oldu. `files` ise bazı motorların doldurduğu ikinci liste.
 *
 * `DataTransferItemList` üzerinde `for...of` yerine indeks kullanılıyor:
 * arayüzün yinelenebilir olması garanti değil.
 */
export function imagesFromClipboard(data: DataTransfer | null): File[] {
  if (!data) return [];

  const found: File[] = [];
  const seen = new Set<string>();

  const take = (file: File | null) => {
    if (!file || !isSupportedImage(file.type)) return;
    // Aynı görüntü hem `files` hem `items` içinde olabiliyor.
    const key = `${file.name}:${file.size}:${file.lastModified}`;
    if (seen.has(key)) return;
    seen.add(key);
    found.push(file);
  };

  for (let i = 0; i < (data.files?.length ?? 0); i += 1) take(data.files.item(i));

  const items = data.items;
  for (let i = 0; i < (items?.length ?? 0); i += 1) {
    const item = items[i];
    if (item && item.kind !== "string") take(item.getAsFile());
  }

  return found;
}

/**
 * Sistem panosundaki görüntüyü dosya olarak okur.
 *
 * Yapıştırma olayı boş geldiğinde son çare — ve WebKitGTK'da bu sık oluyor.
 * İş Rust tarafında yapılıyor: önceki sürüm eklentiden ham RGBA alıp
 * `ImageData` ve canvas üzerinden PNG üretiyordu ve o zincirin dört ayrı
 * yerinde sessizce düşme ihtimali vardı. Artık tek bir çağrı, ve altındaki
 * okuma gerçek panoya karşı ölçülerek doğrulandı.
 */
export async function readClipboardImage(): Promise<File | null> {
  try {
    const image = await api.clipboardImage();
    if (!image) return null;
    return attachmentToFile(image, `pano-${Date.now()}.png`);
  } catch (e) {
    log("warn", "pano görüntüsü okunamadı:", e);
    return null;
  }
}
