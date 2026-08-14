import { useEffect, useRef, useState } from "react";

/**
 * En yavaş yazma hızı (karakter/saniye). Model çok az metin göndermiş olsa
 * bile yazının akıyor gibi görünmesi için bir taban gerekiyor.
 */
const MIN_CPS = 140;

/**
 * En hızlı yazma hızı. Asıl mesele bu: birikmiş metnin oranına göre
 * hızlanmak tek başına yetmiyordu — Haiku gibi hızlı modellerde cevabın
 * tamamı tek karede geldiği için "yazma" değil ışınlanma oluyordu.
 */
const MAX_CPS = 900;

/** Birikmiş metnin yaklaşık ne kadar sürede eritileceği. */
const DRAIN_SECONDS = 0.6;

/**
 * Metni okunabilir bir hızda açar.
 *
 * Gelen metin bir *hedef*; ekrandaki metin her karede ona doğru ilerliyor.
 * Hız geride kalındıkça artıyor ama `MAX_CPS` ile sınırlı: sınır olmadan
 * hızlı modellerde efekt hiç görünmüyordu.
 */
export function useTypewriter(target: string, enabled: boolean): string {
  const [shown, setShown] = useState(target);

  // Hedef bir ref'te tutuluyor; her değişimde rAF döngüsünü yeniden kurmak
  // animasyonu sıfırlar ve titremeye yol açardı.
  const targetRef = useRef(target);
  targetRef.current = target;

  useEffect(() => {
    if (!enabled) {
      setShown(target);
      return;
    }

    let frame = 0;
    let cancelled = false;
    let last = performance.now();
    // Kare başına düşen karakter çoğu zaman kesirli; artan kısım burada
    // birikiyor, yoksa aşağı yuvarlama hızı sistematik olarak düşürürdü.
    let carry = 0;

    function step(now: number) {
      // Sekme arka plandayken rAF durur; dönünce dev bir `dt` gelmesin.
      const dt = Math.min((now - last) / 1000, 0.05);
      last = now;

      setShown((prev) => {
        const goal = targetRef.current;

        // Yeni bir tur başladı: gösterilen metin artık hedefin öneki değil.
        // Hedefe atlamak yerine sıfırdan yazıyoruz — ilk karede tamamına
        // atlamak tam da düzeltmeye çalıştığımız sıçramaydı. İlk render'da bu
        // yola girilmiyor: başlangıç durumu zaten hedefin kendisi, dolayısıyla
        // geçmiş yüklenirken eski mesajlar yeniden yazılmıyor.
        if (!goal.startsWith(prev)) {
          carry = 0;
          return "";
        }

        const remaining = goal.length - prev.length;
        if (remaining === 0) {
          carry = 0;
          return prev;
        }

        const cps = Math.min(Math.max(remaining / DRAIN_SECONDS, MIN_CPS), MAX_CPS);
        carry += cps * dt;

        const chars = Math.floor(carry);
        if (chars < 1) return prev;
        carry -= chars;

        return goal.slice(0, prev.length + Math.min(chars, remaining));
      });

      if (!cancelled) frame = requestAnimationFrame(step);
    }

    frame = requestAnimationFrame(step);
    return () => {
      cancelled = true;
      cancelAnimationFrame(frame);
    };
    // `target` kasıtlı olarak bağımlılık değil; ref üzerinden okunuyor.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [enabled]);

  // Akış kapalıyken hedefi doğrudan göster.
  return enabled ? shown : target;
}
