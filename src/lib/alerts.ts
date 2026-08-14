import {
  isPermissionGranted,
  requestPermission,
  sendNotification,
} from "@tauri-apps/plugin-notification";

import { log } from "@/lib/log";

/** Uyarı verilebilecek olaylar. */
export type AlertEvent = "permission" | "done" | "error" | "question";

export interface AlertRule {
  notify: boolean;
  sound: boolean;
}

export type AlertSettings = Record<AlertEvent, AlertRule>;

export const ALERT_EVENTS: { id: AlertEvent; label: string; hint: string }[] = [
  { id: "permission", label: "İzin isteği", hint: "Claude bir araç çalıştırmak için onay beklediğinde" },
  { id: "question", label: "Soru", hint: "Claude size bir soru sorduğunda" },
  { id: "done", label: "Tamamlandı", hint: "Bir tur bittiğinde ve Claude beklemeye geçtiğinde" },
  { id: "error", label: "Hata", hint: "Oturumda bir hata oluştuğunda" },
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
};

const STORAGE_KEY = "postillion.alerts";

export function loadAlertSettings(): AlertSettings {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (!raw) return DEFAULTS;
    const parsed = JSON.parse(raw) as Partial<AlertSettings>;
    // Eksik anahtarlar varsayılanla tamamlanıyor; ileride yeni olay eklenirse
    // kayıtlı ayar bozulmasın.
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
 * Kısa bir bildirim sesi üretir.
 *
 * Ses dosyası paketlemek yerine Web Audio ile sentezleniyor: tek bir varlık
 * bile eklemeden, her olay için farklı bir ton verilebiliyor.
 */
function tone(frequency: number, duration = 0.12) {
  try {
    audioContext ??= new AudioContext();
    // Tarayıcı otomatik oynatmayı askıya almış olabilir.
    if (audioContext.state === "suspended") void audioContext.resume();

    const now = audioContext.currentTime;
    const oscillator = audioContext.createOscillator();
    const gain = audioContext.createGain();

    oscillator.type = "sine";
    oscillator.frequency.value = frequency;

    // Ani başlangıç ve bitiş "tık" sesi yapıyor; kısa bir zarf yumuşatıyor.
    gain.gain.setValueAtTime(0, now);
    gain.gain.linearRampToValueAtTime(0.09, now + 0.015);
    gain.gain.exponentialRampToValueAtTime(0.0001, now + duration);

    oscillator.connect(gain).connect(audioContext.destination);
    oscillator.start(now);
    oscillator.stop(now + duration + 0.02);
  } catch (e) {
    log("warn", "ses çalınamadı:", e);
  }
}

/** Olaya göre ton; kulak hangi olay olduğunu bakmadan ayırabilsin. */
function playSound(event: AlertEvent) {
  switch (event) {
    case "permission":
      tone(660);
      setTimeout(() => tone(880), 110);
      break;
    case "question":
      tone(740);
      setTimeout(() => tone(988), 110);
      break;
    case "done":
      tone(880, 0.16);
      break;
    case "error":
      tone(300, 0.2);
      break;
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

  if (rule.sound) playSound(event);
  if (rule.notify) void notify(detail.title, detail.body);
}
