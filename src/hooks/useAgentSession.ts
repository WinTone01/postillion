import { useCallback, useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";

import { api, errText, type ImageAttachment } from "@/api";
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
  cwd: string | null;
  /** Devam ettirilecek transcript. */
  resume: string | null;
  /** Transcript dosyasının yolu — geçmiş buradan yükleniyor. */
  transcriptPath: string | null;
  model: string | null;
  /** low | medium | high | xhigh | max. Yalnızca başlatırken geçerli. */
  effort: string | null;
  /**
   * Bu sohbetin kullanacağı MCP sunucuları.
   *
   * `null` genel yapılandırma demek. Liste verildiğinde `--strict-mcp-config`
   * devreye giriyor, yani eklentilerin getirdiği sunucular da kapanıyor.
   */
  mcpServers: string[] | null;
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

/**
 * Bağlam bu eşiği aşınca `/compact` gönderiliyor.
 *
 * Otomatik sıkıştırma interaktif CLI'ın özelliği; headless modda çalışmıyor ve
 * uygulama bunu kendisi yapmazsa bağlam sınırsız büyüyor. Ölçüldü: yalnızca bu
 * uygulamadan sürülen oturumlar hiç sıkışmadan 788k token'a çıkmış, tur başına
 * maliyetleri terminaldeki sıkışan oturumların ~2,5 katıydı — her tur bağlamın
 * tamamını yeniden okuduğu için.
 *
 * 200k, pencere sınırından değil maliyetten seçildi: sınıra kadar beklemek
 * aradaki her turu pahalı yapıyor. `/compact`'in kendi bedeli var (bağlamı bir
 * kez okuyup özet yazıyor), o yüzden eşik daha da düşürülmemeli.
 */
export const AUTO_COMPACT_TOKENS = 200_000;

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

  // `restart` bağımlılıksız bir callback; güncel durumu buradan okuyor.
  const stateRef = useRef(state);
  stateRef.current = state;

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

        // `system/init` canlı bir süreç demek. Yeniden başlatmada eski
        // sürecin `exit` olayı yenisinin başlamasından sonra gelebiliyor ve
        // `running`'i yanlışlıkla kapatıyor; burada kendini toparlıyor.
        if (payload.type === "system" && payload.subtype === "init") {
          setRunning(true);
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
          cwd: opts.cwd,
          resume: opts.resume,
          model: opts.model,
          effort: opts.effort,
          mcpServers: opts.mcpServers,
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

  /**
   * Oturumu farklı bir MCP kümesiyle yeniden başlatır.
   *
   * `--mcp-config` bir başlatma bayrağı ve `/mcp enable|disable` headless
   * modda çalışmıyor ("MCP controls aren't available right now" — ölçüldü),
   * dolayısıyla açık bir sohbetin sunucu kümesini değiştirmenin tek yolu
   * süreci yeniden kurmak. Bağlam kaybolmuyor: `--resume` konuşmayı model
   * tarafında geri getiriyor ve mesajlar zaten arayüzün elinde.
   */
  const restart = useCallback(
    async (mcpServers: string[] | null) => {
      const opts = optionsRef.current;
      if (!opts) return;

      // Süren bir turun ortasında süreci kapatmak cevabı yarıda keser.
      if (stateRef.current.busy) return;

      const sessionId = stateRef.current.sessionId ?? opts.resume;
      if (!sessionId) {
        setState((prev) => ({
          ...prev,
          errors: [...prev.errors, "oturum kimliği yok; yeniden başlatılamıyor"],
        }));
        return;
      }

      setRunning(false);
      try {
        await api.agentStop(opts.id);
        // Yeniden başlatmaya izin ver; geçmiş bayrağı KALIYOR, mesajlar
        // durumda duruyor ve transcript ikinci kez okunmamalı.
        startedAgents.delete(opts.id);

        await api.agentStart({
          id: opts.id,
          cwd: opts.cwd,
          resume: sessionId,
          model: opts.model,
          effort: opts.effort,
          mcpServers,
        });

        startedAgents.add(opts.id);
        setRunning(true);
        log("info", "ajan yeniden başlatıldı", opts.id);
      } catch (e) {
        log("error", "yeniden başlatılamadı:", e);
        setState((prev) => ({ ...prev, errors: [...prev.errors, errText(e)] }));
      }
    },
    [],
  );

  /**
   * Konuşmayı özetleyip bağlamı küçültür.
   *
   * `/compact` headless modda çalışıyor (ölçüldü): `system/status` ile önce
   * `compacting`, sonra sonucu taşıyan bir olay geliyor.
   */
  const compact = useCallback(async () => {
    const opts = optionsRef.current;
    if (!opts || stateRef.current.busy || stateRef.current.compacting) return;
    try {
      await api.agentSend(opts.id, "/compact", []);
    } catch (e) {
      log("error", "sıkıştırılamadı:", e);
    }
  }, []);

  // Eşik aşıldığında bir kez tetiklenmesi için: bağlam ölçüsü sıkıştırma
  // bitene kadar yüksek kalıyor, bayrak olmadan her render yeniden gönderirdi.
  const compactRequestedRef = useRef(false);

  useEffect(() => {
    const context = state.contextTokens;

    // Bağlam eşiğin altına döndü (sıkıştırma tuttu ya da yeni oturum):
    // bir sonraki aşım yeniden tetiklenebilmeli.
    if (context === null || context < AUTO_COMPACT_TOKENS) {
      compactRequestedRef.current = false;
      return;
    }

    if (!running || state.busy || state.compacting || compactRequestedRef.current) return;

    compactRequestedRef.current = true;
    log("info", `bağlam ${context} token — otomatik sıkıştırma`);
    void compact();
  }, [running, state.busy, state.compacting, state.contextTokens, compact]);

  const send = useCallback(
    async (text: string, images: ImageAttachment[] = []) => {
      // Yalnızca görüntüden oluşan bir mesaj da geçerli.
      if (!options || (!text.trim() && images.length === 0)) return;
      // Sıkıştırma sürerken gönderilen bir mesaj sunucu tarafında sıraya
      // giriyor ama arayüz bunu bilmiyor; kullanıcı "gönderdim" sanıp
      // beklerken iki isteğin nasıl iç içe geçtiğini göremiyor. `/compact`
      // kendisi de `send` üzerinden gittiği için burada muaf tutuluyor.
      if (stateRef.current.compacting && text.trim() !== "/compact") return;
      setState((prev) => appendUserMessage(prev, text, images));
      try {
        await api.agentSend(options.id, text, images);
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

  /**
   * `AskUserQuestion` cevabı.
   *
   * Cevap kontrol cevabının `message` alanıyla gidiyor: headless modda
   * tool_result'a ulaşan tek kanal o. Düz "allow" cevabı CLI'a
   * "kullanıcı cevaplamadı" dedirtiyor (ölçüldü).
   */
  const answerQuestions = useCallback(
    async (args: { toolCallId: string; requestId: string; summary: string; message: string }) => {
      if (!options) return;

      setState((prev) => ({
        ...prev,
        messages: prev.messages.map((m) => ({
          ...m,
          parts: m.parts.map((part) =>
            part.kind === "tool" && part.toolCallId === args.toolCallId
              ? {
                  ...part,
                  state: "output-available" as const,
                  output: args.summary,
                  // Arkasından gelen `is_error` tool_result'ın kartı yeniden
                  // açmasını engelliyor.
                  answered: args.summary,
                  permissionRequestId: undefined,
                }
              : part,
          ),
        })),
      }));

      try {
        await api.agentRespondPermission({
          id: options.id,
          requestId: args.requestId,
          allow: false,
          message: args.message,
        });
      } catch (e) {
        log("error", "cevap gönderilemedi:", e);
        setState((prev) => ({ ...prev, errors: [...prev.errors, errText(e)] }));
      }
    },
    [options],
  );

  const setPermissionMode = useCallback(
    async (mode: string) => {
      if (!options) return;
      try {
        await api.agentSetPermissionMode(options.id, mode);
      } catch (e) {
        log("error", "mod değiştirilemedi:", e);
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

  return {
    state,
    running,
    loadingHistory,
    send,
    respondPermission,
    answerQuestions,
    setPermissionMode,
    interrupt,
    restart,
    compact,
  };
}
