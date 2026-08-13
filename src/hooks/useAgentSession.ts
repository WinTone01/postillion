import { useCallback, useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";

import { api, errText } from "@/api";
import { log } from "@/lib/log";
import {
  appendUserMessage,
  initialState,
  reduce,
  seedFromTranscript,
  type SessionState,
} from "@/lib/claude-stream";

interface AgentEvent {
  id: string;
  payload: Record<string, unknown>;
}

interface StderrEvent {
  id: string;
  line: string;
}

interface ExitEvent {
  id: string;
  code: number | null;
}

export interface AgentSessionOptions {
  /** Sekme kimliği; aynı anda birden çok oturum açık olabilir. */
  id: string;
  account: string;
  cwd: string | null;
  /** Devam ettirilecek transcript. */
  resume: string | null;
  /** Transcript dosyasının yolu — geçmiş buradan yükleniyor. */
  transcriptPath: string | null;
  model: string | null;
  /** low | medium | high | xhigh | max. Yalnızca başlatırken geçerli. */
  effort: string | null;
}

/**
 * Bir kez yapılması gereken işler modül seviyesinde izleniyor.
 *
 * Bileşen seviyesinde bir ref yetmez: React StrictMode geliştirmede
 * effect'i çalıştırıp temizleyip tekrar çalıştırıyor. Bu setler o döngüde
 * hayatta kalıyor, böylece süreç iki kez başlamıyor ama dinleyiciler her
 * seferinde yeniden bağlanabiliyor.
 */
const startedAgents = new Set<string>();
const loadedHistories = new Set<string>();

/**
 * Token deltalarının toplu uygulanma aralığı (ms).
 *
 * `--include-partial-messages` saniyede onlarca `stream_event` üretiyor ve her
 * biri ayrı bir React render'ı tetikliyordu. Her render Streamdown'ın markdown'ı
 * baştan ayrıştırmasına yol açtığı için metin akıcı değil, kesik kesik
 * görünüyordu. ~45 ms'lik pencere saniyede ~22 render demek: göz için akıcı,
 * işlemci için ucuz.
 */
const STREAM_FLUSH_MS = 45;

/** Sekme kapanınca çağrılır; aynı kimlik yeniden kullanılabilsin diye. */
export function releaseAgentSession(id: string) {
  startedAgents.delete(id);
  loadedHistories.delete(id);
}

/**
 * Tek bir Claude ajan oturumunu sürer.
 *
 * Dinleyiciler süreç başlatılmadan ÖNCE bağlanır: `listen()` asenkron ve Claude
 * açılır açılmaz `system/init` yayınlıyor. Sıra ters olursa ilk eventler düşer.
 */
export function useAgentSession(options: AgentSessionOptions | null) {
  const [state, setState] = useState<SessionState>(initialState);
  const [running, setRunning] = useState(false);
  const [loadingHistory, setLoadingHistory] = useState(false);

  // Bekleyen akış eventleri ve zamanlayıcısı.
  const bufferRef = useRef<Array<Record<string, unknown>>>([]);
  const flushTimerRef = useRef<number | null>(null);

  const flush = useCallback(() => {
    flushTimerRef.current = null;
    const batch = bufferRef.current;
    if (batch.length === 0) return;
    bufferRef.current = [];
    // Tüm parti tek setState'te; sıra korunuyor.
    setState((prev) => batch.reduce(reduce, prev));
  }, []);

  // Effect yalnızca oturum kimliğine bağlı. `options` nesnesine bağlarsak
  // kimliği değişen her render'da dinleyiciler gereksiz yere sökülür.
  const optionsRef = useRef(options);
  optionsRef.current = options;

  const sessionKey = options?.id ?? null;

  useEffect(() => {
    if (!sessionKey) return;

    // Yalnızca bu effect turuna ait dinleyicileri sökmek için. Kasıtlı olarak
    // ajan başlatmayı ya da geçmiş yüklemeyi ENGELLEMİYOR: StrictMode'un sahte
    // temizliği gerçek bir kapanıştan ayırt edilemez ve bu bayrağı başlatmayı
    // iptal etmek için kullanmak, oturumun hiç başlamamasına yol açar.
    let disposed = false;
    const unlisteners: Array<() => void> = [];

    async function boot() {
      const opts = optionsRef.current!;

      // 1) Dinleyiciler her effect turunda yeniden bağlanır.
      const onEvent = await listen<AgentEvent>("agent://event", (e) => {
        if (e.payload.id !== opts.id) return;

        const payload = e.payload.payload;
        // Her şey tampona giriyor ki olay sırası bozulmasın.
        bufferRef.current.push(payload);

        if (payload.type === "stream_event") {
          // Yalnızca yüksek frekanslı deltalar bekletiliyor.
          if (flushTimerRef.current === null) {
            flushTimerRef.current = window.setTimeout(flush, STREAM_FLUSH_MS);
          }
          return;
        }

        // Tamamlanmış mesaj, izin isteği, tur sonu: gecikmesiz uygulanmalı.
        if (flushTimerRef.current !== null) {
          clearTimeout(flushTimerRef.current);
          flushTimerRef.current = null;
        }
        flush();
      });
      const onStderr = await listen<StderrEvent>("agent://stderr", (e) => {
        if (e.payload.id !== opts.id) return;
        setState((prev) => ({ ...prev, errors: [...prev.errors, e.payload.line] }));
      });
      const onExit = await listen<ExitEvent>("agent://exit", (e) => {
        if (e.payload.id !== opts.id) return;
        setRunning(false);
        setState((prev) => ({ ...prev, busy: false }));
      });

      if (disposed) {
        // Bu tur bitmiş; sadece bağladıklarımızı geri al.
        onEvent();
        onStderr();
        onExit();
        return;
      }
      unlisteners.push(onEvent, onStderr, onExit);

      // 2) Geçmişi yükle — oturum başına bir kez.
      //
      // `claude --resume` geçmişi tekrar yayınlamıyor (ölçüldü: boş stdin ile
      // sıfır satır), oturum yalnızca model tarafında geri geliyor.
      if (opts.transcriptPath && !loadedHistories.has(opts.id)) {
        loadedHistories.add(opts.id);
        setLoadingHistory(true);
        try {
          const records = await api.readTranscript(opts.transcriptPath);
          log("info", `geçmiş okundu: ${records.length} kayıt`);
          // Handshake cevabı önce gelmiş olabilir; komut ve model listesini
          // geçmiş yüklemesi ezmemeli.
          setState((prev) => ({
            ...seedFromTranscript(records),
            commands: prev.commands,
            models: prev.models,
          }));
        } catch (e) {
          log("error", "geçmiş okunamadı:", e);
          loadedHistories.delete(opts.id);
          setState((prev) => ({
            ...prev,
            errors: [...prev.errors, `geçmiş yüklenemedi: ${errText(e)}`],
          }));
        } finally {
          setLoadingHistory(false);
        }
      }

      // 3) Ajanı başlat — oturum başına bir kez.
      if (startedAgents.has(opts.id)) {
        setRunning(true);
        return;
      }
      startedAgents.add(opts.id);

      try {
        await api.agentStart({
          id: opts.id,
          account: opts.account,
          cwd: opts.cwd,
          resume: opts.resume,
          model: opts.model,
          effort: opts.effort,
        });
        log("info", "ajan başlatıldı", opts.id);
        setRunning(true);
      } catch (e) {
        log("error", "ajan başlatılamadı:", e);
        startedAgents.delete(opts.id);
        setState((prev) => ({ ...prev, errors: [...prev.errors, errText(e)] }));
      }
    }

    void boot().catch((e) => {
      log("error", "boot çöktü:", e);
      setState((prev) => ({
        ...prev,
        busy: false,
        errors: [...prev.errors, `oturum başlatılamadı: ${errText(e)}`],
      }));
    });

    return () => {
      disposed = true;
      if (flushTimerRef.current !== null) {
        clearTimeout(flushTimerRef.current);
        flushTimerRef.current = null;
      }
      unlisteners.forEach((un) => un());
    };
  }, [sessionKey, flush]);

  const send = useCallback(
    async (text: string) => {
      if (!options || !text.trim()) return;
      setState((prev) => appendUserMessage(prev, text));
      try {
        await api.agentSend(options.id, text);
      } catch (e) {
        setState((prev) => ({
          ...prev,
          busy: false,
          errors: [...prev.errors, errText(e)],
        }));
      }
    },
    [options],
  );

  const respondPermission = useCallback(
    async (args: {
      toolCallId: string;
      requestId: string;
      allow: boolean;
      /** Verilirse izin modunu da değiştirir ("hep izin ver"). */
      setMode?: string;
    }) => {
      if (!options) return;

      // Arayüzü hemen güncelle; cevabı beklemek butonu donuk gösterir.
      setState((prev) => ({
        ...prev,
        messages: prev.messages.map((message) => ({
          ...message,
          parts: message.parts.map((part) =>
            part.kind === "tool" && part.toolCallId === args.toolCallId
              ? {
                  ...part,
                  state: args.allow
                    ? ("input-available" as const)
                    : ("output-denied" as const),
                  approved: args.allow,
                  permissionRequestId: undefined,
                }
              : part,
          ),
        })),
      }));

      try {
        if (args.setMode) {
          await api.agentSetPermissionMode(options.id, args.setMode);
        }
        await api.agentRespondPermission({
          id: options.id,
          requestId: args.requestId,
          allow: args.allow,
        });
      } catch (e) {
        log("error", "izin cevabı gönderilemedi:", e);
        setState((prev) => ({ ...prev, errors: [...prev.errors, errText(e)] }));
      }
    },
    [options],
  );

  const interrupt = useCallback(async () => {
    if (!options) return;
    try {
      await api.agentInterrupt(options.id);
    } catch (e) {
      setState((prev) => ({ ...prev, errors: [...prev.errors, errText(e)] }));
    }
  }, [options]);

  return { state, running, loadingHistory, send, respondPermission, interrupt };
}
