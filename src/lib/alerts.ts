import {
  isPermissionGranted,
  requestPermission,
  sendNotification,
} from "@tauri-apps/plugin-notification";

import { log } from "@/lib/log";
import { t } from "@/lib/i18n";

/** Uyarı verilebilecek olaylar. */
export type AlertEvent = "permission" | "done" | "error" | "question";

export interface AlertRule {
  notify: boolean;
  sound: boolean;
}

export type AlertSettings = Record<AlertEvent, AlertRule> & {
  /** 0–1 arası ses düzeyi. */
  volume: number;
};

export const ALERT_EVENTS: { id: AlertEvent; label: string; hint: string }[] = [
  {
    id: "permission",
    label: t("İzin isteği"),
    hint: t("Claude bir araç çalıştırmak için onay beklediğinde"),
  },
  { id: "question", label: t("Soru"), hint: t("Claude size bir soru sorduğunda") },
  {
    id: "done",
    label: t("Tamamlandı"),
    hint: t("Bir tur bittiğinde ve Claude beklemeye geçtiğinde"),
  },
  { id: "error", label: t("Hata"), hint: t("Oturumda bir hata oluştuğunda") },
];

/**
 * Varsayılanlar: dikkat gerektiren iki olayda hem bildirim hem ses.
 * Tamamlanma yalnızca ses — uzun işlerde kullanışlı ama bildirim kadar
 * araya girmesine gerek yok.
 */
const DEFAULTS: AlertSettings = {
  permission: { notify: true, sound: true },
  question: { notify: true, sound: true },
  done: { notify: false, sound: true },
  error: { notify: false, sound: false },
  volume: 0.7,
};

const STORAGE_KEY = "postillion.alerts";

export function loadAlertSettings(): AlertSettings {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (!raw) return DEFAULTS;
    const parsed = JSON.parse(raw) as Partial<AlertSettings>;
    // Eksik anahtarlar varsayılanla tamamlanıyor: hem ileride yeni bir olay
    // eklendiğinde hem de `volume` gibi sonradan gelen alanlarda kayıtlı ayar
    // bozulmasın.
    return { ...DEFAULTS, ...parsed };
  } catch {
    return DEFAULTS;
  }
}

export function saveAlertSettings(settings: AlertSettings) {
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(settings));
  } catch (e) {
    log("warn", "uyarı ayarları kaydedilemedi:", e);
  }
}

// ------------------------------------------------------------------- ses

let audioContext: AudioContext | null = null;

/**
 * Tepe kazanç.
 *
 * Önceki 0.09 masaüstü bildirim seslerinin yanında duyulmayacak kadar
 * kısıktı. Bu değer ayardaki ses düzeyiyle çarpılıyor.
 */
const PEAK_GAIN = 0.55;

/**
 * Kısa bir bildirim sesi üretir.
 *
 * Ses dosyası paketlemek yerine Web Audio ile sentezleniyor: tek bir varlık
 * bile eklemeden, her olay için farklı bir ton verilebiliyor.
 *
 * Saf sinüs zayıf duyuluyordu; üstüne kısık bir üçgen dalga bindiriliyor.
 * Üst harmonikler sesi hoparlörde belirgin kılıyor, gürültülü yapmadan.
 */
function tone(frequency: number, volume: number, duration = 0.12) {
  try {
    audioContext ??= new AudioContext();
    // Tarayıcı otomatik oynatmayı askıya almış olabilir.
    if (audioContext.state === "suspended") void audioContext.resume();

    const now = audioContext.currentTime;
    const peak = Math.max(0, Math.min(1, volume)) * PEAK_GAIN;
    if (peak === 0) return;

    const gain = audioContext.createGain();
    // Ani başlangıç ve bitiş "tık" sesi yapıyor; kısa bir zarf yumuşatıyor.
    gain.gain.setValueAtTime(0.0001, now);
    gain.gain.linearRampToValueAtTime(peak, now + 0.012);
    gain.gain.exponentialRampToValueAtTime(0.0001, now + duration);
    gain.connect(audioContext.destination);

    for (const [type, mix] of [
      ["sine", 1],
      ["triangle", 0.35],
    ] as const) {
      const oscillator = audioContext.createOscillator();
      const level = audioContext.createGain();
      oscillator.type = type;
      oscillator.frequency.value = frequency;
      level.gain.value = mix;
      oscillator.connect(level).connect(gain);
      oscillator.start(now);
      oscillator.stop(now + duration + 0.02);
    }
  } catch (e) {
    log("warn", "ses çalınamadı:", e);
  }
}

/** Olaya göre ton; kulak hangi olay olduğunu bakmadan ayırabilsin. */
function playSound(event: AlertEvent, volume: number) {
  switch (event) {
    case "permission":
      tone(660, volume);
      setTimeout(() => tone(880, volume), 110);
      break;
    case "question":
      tone(740, volume);
      setTimeout(() => tone(988, volume), 110);
      break;
    case "done":
      tone(880, volume, 0.16);
      break;
    case "error":
      tone(300, volume, 0.2);
      break;
  }
}

/**
 * Ses bağlamını bir kullanıcı hareketiyle uyandırır.
 *
 * Otomatik oynatma politikası yüzünden ilk ses, kullanıcı sayfaya hiç
 * dokunmadıysa sessizce düşüyor. Uygulama içindeki ilk tıklamada çağrılıyor.
 */
export function primeAudio() {
  try {
    audioContext ??= new AudioContext();
    if (audioContext.state === "suspended") void audioContext.resume();
  } catch {
    // Ses yoksa uygulama yine de çalışmalı.
  }
}

// -------------------------------------------------------------- bildirim

let permissionGranted: boolean | null = null;

async function notify(title: string, body: string) {
  try {
    permissionGranted ??= await isPermissionGranted();
    if (!permissionGranted) {
      permissionGranted = (await requestPermission()) === "granted";
    }
    if (!permissionGranted) return;
    sendNotification({ title, body });
  } catch (e) {
    log("warn", "bildirim gönderilemedi:", e);
  }
}

/** Bir olay için ayarlarda etkin olan uyarıları verir. */
export function fireAlert(
  settings: AlertSettings,
  event: AlertEvent,
  detail: { title: string; body: string },
) {
  const rule = settings[event];
  if (!rule) return;

  if (rule.sound) playSound(event, settings.volume ?? 1);
  if (rule.notify) void notify(detail.title, detail.body);
}
