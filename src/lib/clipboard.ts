import { readImage, writeText } from "@tauri-apps/plugin-clipboard-manager";

import { isSupportedImage } from "@/lib/images";
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

/** Yapıştırılan şey düz metin mi — öyleyse panoyu ayrıca yoklamaya gerek yok. */
export function looksLikeText(data: DataTransfer | null): boolean {
  const types = data?.types;
  if (!types || types.length === 0) return false;
  return [...types].some((type) => type === "text/plain") &&
    ![...types].some((type) => type.startsWith("image/"));
}

/**
 * Sistem panosundaki görüntüyü PNG dosyası olarak okur.
 *
 * Yapıştırma olayı boş geldiğinde son çare. Tauri eklentisi ham RGBA veriyor,
 * PNG'ye çevirmek için canvas gerekiyor.
 */
export async function readClipboardImage(): Promise<File | null> {
  try {
    const image = await readImage();
    const { width, height } = await image.size();
    if (width === 0 || height === 0) return null;

    const rgba = await image.rgba();
    const canvas = document.createElement("canvas");
    canvas.width = width;
    canvas.height = height;

    const context = canvas.getContext("2d");
    if (!context) return null;
    context.putImageData(new ImageData(new Uint8ClampedArray(rgba), width, height), 0, 0);

    const blob = await new Promise<Blob | null>((resolve) =>
      canvas.toBlob(resolve, "image/png"),
    );
    if (!blob) return null;

    return new File([blob], `pano-${Date.now()}.png`, { type: "image/png" });
  } catch (e) {
    // Panoda görüntü yoksa eklenti hata veriyor; bu beklenen bir durum.
    log("info", "panoda görüntü yok:", e);
    return null;
  }
}
