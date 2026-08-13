import { invoke } from "@tauri-apps/api/core";

/**
 * Webview günlüklerini Rust sürecinin stderr'ine aktarır.
 *
 * Tauri penceresinde konsola ulaşmak için devtools açmak gerekiyor; bu köprü
 * sayesinde hatalar doğrudan `npm run tauri dev` çıktısında görünüyor.
 */
export function log(level: "info" | "warn" | "error", ...parts: unknown[]) {
  const message = parts
    .map((p) => {
      if (typeof p === "string") return p;
      if (p instanceof Error) return `${p.name}: ${p.message}`;
      try {
        return JSON.stringify(p);
      } catch {
        return String(p);
      }
    })
    .join(" ");

  // Konsola da yaz; devtools açıksa orada da görünsün.
  if (level === "error") console.error(message);
  else console.warn(message);

  // Köprünün kendisi patlarsa sessizce vazgeç — sonsuz döngü olmasın.
  invoke("log_frontend", { level, message }).catch(() => {});
}

/** Yakalanmamış hataları da köprüye bağlar. */
export function installGlobalLogging() {
  window.addEventListener("error", (event) => {
    log("error", "uncaught:", event.message, "@", `${event.filename}:${event.lineno}`);
  });

  window.addEventListener("unhandledrejection", (event) => {
    log("error", "unhandled rejection:", event.reason);
  });
}
