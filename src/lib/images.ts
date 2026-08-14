import type { ImageAttachment } from "@/api";

/** Claude'un kabul ettiği görüntü biçimleri. */
const SUPPORTED = new Set(["image/png", "image/jpeg", "image/gif", "image/webp"]);

export function isSupportedImage(mediaType: string | undefined): boolean {
  return SUPPORTED.has((mediaType ?? "").toLowerCase());
}

/**
 * Bir `blob:` ya da `data:` URL'ini base64 eke çevirir.
 *
 * `FileReader` yerine `fetch` + manuel kodlama: `readAsDataURL` sonucundan
 * öneki kırpmak da aynı işi yapardı ama büyük görüntülerde iki kez dize
 * kopyalıyordu.
 */
export async function urlToAttachment(
  url: string,
  mediaType: string,
): Promise<ImageAttachment | null> {
  if (!isSupportedImage(mediaType)) return null;

  const bytes = new Uint8Array(await (await fetch(url)).arrayBuffer());
  return { mediaType, data: encodeBase64(bytes) };
}

/** Base64 eki tekrar dosyaya çevirir; ek listesi `File` bekliyor. */
export function attachmentToFile(attachment: ImageAttachment, name: string): File {
  const binary = atob(attachment.data);
  const bytes = new Uint8Array(binary.length);
  for (let i = 0; i < binary.length; i += 1) bytes[i] = binary.charCodeAt(i);
  return new File([bytes], name, { type: attachment.mediaType });
}

/**
 * Base64 kodlar.
 *
 * `btoa(String.fromCharCode(...bytes))` tek satır olurdu ama birkaç yüz KB'lık
 * bir görüntüde argüman sayısı yığını taşırıyor; parça parça ilerliyoruz.
 */
function encodeBase64(bytes: Uint8Array): string {
  const CHUNK = 0x8000;
  let binary = "";
  for (let i = 0; i < bytes.length; i += CHUNK) {
    binary += String.fromCharCode(...bytes.subarray(i, i + CHUNK));
  }
  return btoa(binary);
}
