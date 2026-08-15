import { motion, type Variants } from "motion/react";

/**
 * Uygulamanın maskotu: küçük bir postacı.
 *
 * Tamamen `<rect>` ile çiziliyor — eğri ya da path yok. Piksel sanatı
 * yaklaşımı küçük boyutlarda (kenar çubuğunda 32 px) keskin kalıyor ve her
 * parça bağımsız animasyon alabiliyor.
 *
 * Durumlar uygulamanın gerçek durumunu yansıtıyor; süsleme değil, göstergedir.
 */
export type MascotState = "idle" | "thinking" | "working" | "waiting" | "error";

interface Props {
  state?: MascotState;
  className?: string;
  /** Erişilebilirlik etiketi; `null` ise süs kabul edilir. */
  label?: string | null;
}

/** Gövdenin bütünü: nefes alma, öne eğilme, çökme. */
const bodyVariants: Variants = {
  // Boşta hareket YOK. Ölçüldü: sürekli dönen animasyonlar uygulamayı boşta
  // bir çekirdeğin ~%70'inde tutuyordu, çünkü NVIDIA + Wayland'de DMABUF
  // renderer kapalı ve her kare işlemcide birleştiriliyor. Boştaki kıpırtı
  // bilgi taşımıyordu; hareket artık yalnızca bir şey olduğunda var.
  idle: { y: 0, rotate: 0, transition: { duration: 0.4 } },
  thinking: {
    // Sallanma: "düşünüyor" hissini veren şey ritmin yavaşlaması.
    y: [0, -1, 0],
    rotate: [-7, 7, -7],
    transition: { duration: 2.8, repeat: Infinity, ease: "easeInOut" },
  },
  working: {
    // Zıplama; koşan bir postacı ritmi.
    y: [0, -2.6, 0],
    rotate: [-3, 3, -3],
    transition: { duration: 0.5, repeat: Infinity, ease: "easeInOut" },
  },
  waiting: {
    // Yerinde duramama: kısa, ısrarlı bir hatırlatma.
    y: [0, -0.9, 0],
    rotate: [-4, 4, -4],
    transition: { duration: 0.75, repeat: Infinity, ease: "easeInOut" },
  },
  error: {
    y: 2.2,
    rotate: -9,
    transition: { type: "spring", stiffness: 260, damping: 14 },
  },
};

/** Kollar: yalnızca çalışırken belirgin biçimde hareket ediyor. */
const armVariants: Variants = {
  idle: { rotate: 0, transition: { duration: 0.4 } },
  thinking: { rotate: 0, transition: { duration: 0.4 } },
  working: {
    rotate: [-32, 32, -32],
    transition: { duration: 0.5, repeat: Infinity, ease: "easeInOut" },
  },
  waiting: {
    rotate: [-10, 10, -10],
    transition: { duration: 0.75, repeat: Infinity, ease: "easeInOut" },
  },
  error: { rotate: 14, transition: { duration: 0.35 } },
};

/**
 * Gözler.
 *
 * Kırpma `scaleY` ile yapılıyor: bekleme süresi uzun, kapanma çok kısa —
 * gerçekçi olan bu oran. Sabit aralık yerine uzun bir döngü içinde iki hızlı
 * kırpma, mekanik görünmesini engelliyor.
 */
const eyeVariants: Variants = {
  // Kırpma boştayken CSS'e devrediliyor: tek bir bileşik animasyon, kare
  // başına JS yok. Karakter canlı kalıyor, döngü sürmüyor.
  idle: { scaleY: 1, y: 0, transition: { duration: 0.3 } },
  thinking: {
    // Yukarı bakış: düşünmenin evrensel görsel kısaltması.
    scaleY: [1, 0.7, 1],
    y: -1,
    transition: { duration: 2.4, repeat: Infinity, ease: "easeInOut" },
  },
  working: {
    scaleY: [1, 0.55, 1],
    y: 0,
    transition: { duration: 0.55, repeat: Infinity, ease: "easeInOut" },
  },
  waiting: {
    // Büyümüş gözler: "senden bir şey bekliyorum".
    scaleY: 1.5,
    y: -0.3,
    transition: { duration: 0.25, ease: "easeOut" },
  },
  error: {
    scaleY: 0.25,
    y: 0.4,
    transition: { duration: 0.3 },
  },
};

export default function Mascot({ state = "idle", className, label }: Props) {
  return (
    <svg
      aria-hidden={label === null || label === undefined ? true : undefined}
      aria-label={label ?? undefined}
      className={className}
      fill="none"
      role={label ? "img" : undefined}
      viewBox="0 0 20 20"
      xmlns="http://www.w3.org/2000/svg"
    >
      <motion.g animate={state} variants={bodyVariants}>
        {/* Kasket — postacıyı postacı yapan detay. */}
        <rect fill="var(--mascot-ink)" height="2.6" rx="0.3" width="9" x="5.5" y="1.4" />
        <rect fill="var(--mascot-ink)" height="1.2" rx="0.3" width="13" x="3.5" y="3.8" />

        {/* Baş */}
        <rect fill="var(--mascot-ink)" height="6.6" rx="0.6" width="9" x="5.5" y="5.2" />

        {/* Gözler; ayrı grup, çünkü kırpma yalnızca onları etkiliyor.
            Boştaki kırpma CSS'te: kare başına JS çalıştırmadan canlı kalıyor. */}
        <motion.g
          animate={state}
          className={state === "idle" ? "cs-mascot-blink" : undefined}
          variants={eyeVariants}
        >
          <rect
            fill="var(--mascot-cutout)"
            height="2.4"
            rx="0.35"
            width="1.7"
            x="7.4"
            y="7.6"
          />
          <rect
            fill="var(--mascot-cutout)"
            height="2.4"
            rx="0.35"
            width="1.7"
            x="10.9"
            y="7.6"
          />
        </motion.g>

        {/* Gövde */}
        <rect fill="var(--mascot-ink)" height="4.6" rx="0.5" width="7.4" x="6.3" y="12.4" />

        {/* Çanta askısı — gövdeyi ikiye bölerek siluete okunurluk katıyor. */}
        <rect
          fill="var(--mascot-cutout)"
          height="4.6"
          opacity="0.45"
          width="0.9"
          x="8.4"
          y="12.4"
        />

        {/* Kollar. transformOrigin omuzda: dönme oradan olmalı, yoksa kol
            gövdeden kopuk savruluyor. */}
        <motion.rect
          animate={state}
          fill="var(--mascot-ink)"
          height="3.6"
          rx="0.45"
          style={{ transformOrigin: "5.2px 13px" }}
          variants={armVariants}
          width="1.8"
          x="4.3"
          y="12.6"
        />
        <motion.rect
          animate={state}
          fill="var(--mascot-ink)"
          height="3.6"
          rx="0.45"
          style={{ transformOrigin: "14.8px 13px" }}
          variants={{
            ...armVariants,
            // Karşı kol ters fazda; aynı yönde sallanınca robot gibi duruyor.
            working: {
              rotate: [32, -32, 32],
              transition: { duration: 0.5, repeat: Infinity, ease: "easeInOut" },
            },
            waiting: {
              rotate: [10, -10, 10],
              transition: { duration: 0.75, repeat: Infinity, ease: "easeInOut" },
            },
          }}
          width="1.8"
          x="13.9"
          y="12.6"
        />

        {/* Ayaklar */}
        <rect fill="var(--mascot-ink)" height="1.7" rx="0.4" width="2.6" x="6.5" y="17.3" />
        <rect fill="var(--mascot-ink)" height="1.7" rx="0.4" width="2.6" x="10.9" y="17.3" />
      </motion.g>
    </svg>
  );
}
