import { useEffect } from "react";
import { XIcon } from "lucide-react";

import { t } from "@/lib/i18n";

interface Props {
  /** Gösterilecek görüntünün URL'i; `null` ise kapalı. */
  src: string | null;
  alt?: string;
  onClose: () => void;
}

/**
 * Tam ekran görüntü önizlemesi.
 *
 * Küçük resimlerden hangisinin ne olduğu 56 pikselde anlaşılmıyor; tıklayınca
 * büyümesi bekleniyor. Radix diyaloğu yerine sade bir katman: burada odak
 * tuzağına ya da form anlamına gerek yok, tek iş görüntüyü büyütmek.
 */
export default function ImageViewer({ src, alt, onClose }: Props) {
  useEffect(() => {
    if (!src) return;

    function onKey(event: KeyboardEvent) {
      if (event.key === "Escape") onClose();
    }

    document.addEventListener("keydown", onKey);
    return () => document.removeEventListener("keydown", onKey);
  }, [src, onClose]);

  if (!src) return null;

  return (
    // Katmanın kendisine tıklamak kapatıyor; görüntüye tıklamak kapatmıyor.
    <div
      className="cs-viewer fixed inset-0 z-[60] flex items-center justify-center p-8"
      onClick={onClose}
      role="presentation"
    >
      <img
        alt={alt ?? t("İliştirilen görüntü")}
        className="max-h-full max-w-full rounded-lg object-contain shadow-2xl"
        onClick={(event) => event.stopPropagation()}
        src={src}
      />

      <button
        aria-label={t("Kapat")}
        className="absolute top-4 right-4 grid size-9 place-items-center rounded-full bg-background/80 text-muted-foreground backdrop-blur transition-colors hover:text-foreground"
        onClick={onClose}
        type="button"
      >
        <XIcon className="size-4" />
      </button>
    </div>
  );
}
