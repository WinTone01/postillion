import { useEffect, useRef, useState } from "react";

/**
 * Metni sabit bir hızda açar.
 *
 * Neden gerekli: model token'ları düzensiz büyüklükte parçalar hâlinde
 * gönderiyor — bazen bir kelime, bazen bir paragraf. Geleni olduğu gibi
 * basmak metni sıçratıyordu. Burada gelen metin bir hedef olarak tutuluyor ve
 * ekrandaki metin her karede ona doğru yaklaşıyor.
 *
 * Geride kalındıkça hız artıyor: sabit hız, model bizden hızlıysa yazının
 * giderek gerisine düşmesine yol açardı.
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

    function step() {
      setShown((prev) => {
        const goal = targetRef.current;

        // Yeni mesaj başladı ya da metin kısaldı: baştan başla.
        if (!goal.startsWith(prev)) return goal;
        if (prev.length >= goal.length) return prev;

        const remaining = goal.length - prev.length;
        // Kalanın bir bölümü kadar ilerle; böylece hem akıcı hem de
        // büyük bir parça geldiğinde hızla yetişiyor.
        const chunk = Math.max(2, Math.ceil(remaining / 10));
        return goal.slice(0, prev.length + chunk);
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
