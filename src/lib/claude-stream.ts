/**
 * Claude Code'un stream-json akışını arayüz modeline çevirir.
 *
 * Akıştaki event tipleri (deneyle ölçüldü):
 *   system/init            oturum kimliği, model, araç listesi
 *   stream_event/*         token-token deltalar (--include-partial-messages)
 *   assistant              tamamlanmış mesaj: text + tool_use blokları
 *   user                   tool_result blokları
 *   control_request        can_use_tool → izin bekliyor, cevabımızı istiyor
 *   result/success         tur bitti, usage ve maliyet
 *
 * Tasarım kararı: deltaları ve tam mesajları birleştirmeye çalışmıyoruz.
 * `assistant` event'i tek doğru kaynak; deltalar yalnızca "yazıyor…"
 * önizlemesini besliyor ve tam mesaj gelince atılıyor. Aksi halde aynı metin
 * iki kez birikir.
 */

/** AI Elements'in Tool bileşeninin beklediği durumlar. */
export type ToolState =
  | "input-streaming"
  | "input-available"
  | "approval-requested"
  | "approval-responded"
  | "output-available"
  | "output-error"
  | "output-denied";

export interface ToolPart {
  kind: "tool";
  toolCallId: string;
  name: string;
  input: unknown;
  state: ToolState;
  output?: unknown;
  errorText?: string;
  /** İzin bekleyen araçlarda cevap için gereken kontrol isteği kimliği. */
  permissionRequestId?: string;
  /** Kullanıcının kararı — Confirmation bileşeni bunu okuyor. */
  approved?: boolean;
  /** CLI'ın önerdiği kısayollar, ör. "hep izin ver". */
  suggestions?: PermissionSuggestion[];
  description?: string;
}

export interface TextPart {
  kind: "text";
  text: string;
}

/** Modelin düşünme bloğu; Claude Desktop'taki gibi katlanır gösteriliyor. */
export interface ThinkingPart {
  kind: "thinking";
  text: string;
}

export type Part = TextPart | ThinkingPart | ToolPart;

export interface ChatMessage {
  id: string;
  role: "user" | "assistant";
  parts: Part[];
}

export interface PermissionSuggestion {
  type: string;
  mode?: string;
  destination?: string;
}

/** `initialize` handshake'inden gelen slash komutu. */
export interface SlashCommand {
  name: string;
  description: string;
}

/** `initialize` handshake'inden gelen model seçeneği. */
export interface ModelInfo {
  value: string;
  displayName: string;
  description?: string;
  resolvedModel?: string;
}

/** Rust tarafındaki `agent::INIT_REQUEST_ID` ile eşleşmeli. */
export const INIT_REQUEST_ID = "cs-initialize";

export interface SessionState {
  messages: ChatMessage[];
  /** Akmakta olan asistan metni; `assistant` event'i gelince temizlenir. */
  streamingText: string;
  /** Model bir tur yürütüyor mu. */
  busy: boolean;
  sessionId: string | null;
  model: string | null;
  cwd: string | null;
  /** Uygulama içi teşhis günlüğü. */
  errors: string[];
  totalCostUsd: number | null;
  /** Slash komutları — yalnızca handshake cevabından öğrenilebiliyor. */
  commands: SlashCommand[];
  /** Modeller; katalogdaki elle listeden daha doğru. */
  models: ModelInfo[];
}

export const initialState: SessionState = {
  messages: [],
  streamingText: "",
  busy: false,
  sessionId: null,
  model: null,
  cwd: null,
  errors: [],
  totalCostUsd: null,
  commands: [],
  models: [],
};

type Json = Record<string, unknown>;

function asArray(value: unknown): Json[] {
  return Array.isArray(value) ? (value as Json[]) : [];
}

function parseCommands(value: unknown): SlashCommand[] {
  return asArray(value)
    .map((item) => ({
      name: String(item.name ?? ""),
      description: String(item.description ?? ""),
    }))
    .filter((c) => c.name.length > 0);
}

function parseModels(value: unknown): ModelInfo[] {
  return asArray(value)
    .map((item) => ({
      value: String(item.value ?? ""),
      displayName: String(item.displayName ?? item.value ?? ""),
      description: item.description ? String(item.description) : undefined,
      resolvedModel: item.resolvedModel ? String(item.resolvedModel) : undefined,
    }))
    .filter((m) => m.value.length > 0);
}

/** Tool part'ı tüm mesajlar içinde toolCallId ile bulup değiştirir. */
function patchTool(
  messages: ChatMessage[],
  toolCallId: string,
  patch: Partial<ToolPart>,
): ChatMessage[] {
  let found = false;

  const next = messages.map((message) => {
    if (found) return message;

    const parts = message.parts.map((part) => {
      if (part.kind !== "tool" || part.toolCallId !== toolCallId) return part;
      found = true;
      return { ...part, ...patch };
    });

    return found ? { ...message, parts } : message;
  });

  return found ? next : messages;
}

/**
 * Aynı mesaja ait parçaları birleştirir.
 *
 * Araçlar `toolCallId` ile tekilleştirilir — aynı `tool_use` birden çok kayıtta
 * görünebiliyor ve iki kez çizilmemeli. Metinlerde ise yalnızca bitişik birebir
 * tekrar elenir; aynı metin bloğunun tekrar gönderilmesi metni ikiye katlardı.
 */
function mergeParts(existing: Part[], incoming: Part[]): Part[] {
  const seenTools = new Set(
    existing.filter((p): p is ToolPart => p.kind === "tool").map((p) => p.toolCallId),
  );

  const merged = [...existing];

  for (const part of incoming) {
    if (part.kind === "tool") {
      if (seenTools.has(part.toolCallId)) continue;
      seenTools.add(part.toolCallId);
      merged.push(part);
      continue;
    }

    const last = merged[merged.length - 1];
    if (last?.kind === part.kind && "text" in last && last.text === part.text) continue;
    merged.push(part);
  }

  return merged;
}

let counter = 0;
function nextId(prefix: string): string {
  counter += 1;
  return `${prefix}-${counter}`;
}

/**
 * Tek bir akış event'ini duruma uygular.
 *
 * Saf fonksiyon: aynı girdi her zaman aynı çıktıyı verir, bu yüzden
 * test edilebilir.
 */
export function reduce(state: SessionState, event: Json): SessionState {
  const type = event.type as string | undefined;

  switch (type) {
    // Handshake cevabı: slash komutları ve modeller burada geliyor.
    case "control_response": {
      const envelope = event.response as Json | undefined;
      if (envelope?.request_id !== INIT_REQUEST_ID) return state;

      const body = (envelope.response as Json | undefined) ?? envelope;
      return {
        ...state,
        commands: parseCommands(body.commands),
        models: parseModels(body.models),
      };
    }

    case "system": {
      // Eklenti açılıp kapandığında komut listesi değişiyor.
      if (event.subtype === "commands_changed") {
        return { ...state, commands: parseCommands(event.commands) };
      }

      if (event.subtype === "init") {
        return {
          ...state,
          sessionId: (event.session_id as string) ?? state.sessionId,
          model: (event.model as string) ?? state.model,
          cwd: (event.cwd as string) ?? state.cwd,
          busy: true,
        };
      }
      return state;
    }

    case "stream_event": {
      const inner = event.event as Json | undefined;
      const innerType = inner?.type as string | undefined;

      if (innerType === "message_start") {
        return { ...state, streamingText: "", busy: true };
      }

      if (innerType === "content_block_delta") {
        const delta = inner?.delta as Json | undefined;
        if (delta?.type === "text_delta" && typeof delta.text === "string") {
          return { ...state, streamingText: state.streamingText + delta.text };
        }
      }

      return state;
    }

    case "assistant": {
      const message = event.message as Json | undefined;
      const content = asArray(message?.content);

      const parts: Part[] = [];
      for (const block of content) {
        if (block.type === "text" && typeof block.text === "string") {
          parts.push({ kind: "text", text: block.text });
        } else if (block.type === "thinking" && typeof block.thinking === "string") {
          parts.push({ kind: "thinking", text: block.thinking });
        } else if (block.type === "tool_use") {
          parts.push({
            kind: "tool",
            toolCallId: String(block.id ?? nextId("tool")),
            name: String(block.name ?? "tool"),
            input: block.input,
            // İzin isteği gelirse durum approval-requested'a geçecek.
            state: "input-available",
          });
        }
      }

      if (parts.length === 0) {
        return { ...state, streamingText: "" };
      }

      const id = String(message?.id ?? nextId("msg"));
      const last = state.messages[state.messages.length - 1];

      // Claude tek bir asistan mesajını birden çok kayda bölüyor: aynı
      // `message.id` gerçek transcript'lerde 6 kez tekrar edebiliyor. Ayrı
      // mesajlar olarak eklersek React aynı `key`'i görüp çocukları atıyor ve
      // sohbetin yarısı kayboluyor. Doğrusu birleştirmek.
      if (last && last.role === "assistant" && last.id === id) {
        return {
          ...state,
          streamingText: "",
          messages: [
            ...state.messages.slice(0, -1),
            { ...last, parts: mergeParts(last.parts, parts) },
          ],
        };
      }

      return {
        ...state,
        streamingText: "",
        messages: [...state.messages, { id, role: "assistant", parts }],
      };
    }

    case "user": {
      const message = event.message as Json | undefined;
      const content = message?.content;

      // Düz metin: gerçek bir kullanıcı mesajı. Canlı akışta bu hiç gelmiyor
      // (kullanıcı mesajını iyimser olarak biz ekliyoruz), ama transcript'ten
      // geçmiş yüklerken kayıtların çoğu bu biçimde.
      if (typeof content === "string") {
        if (!content.trim()) return state;
        return {
          ...state,
          messages: [
            ...state.messages,
            {
              id: String(event.uuid ?? nextId("user")),
              role: "user",
              parts: [{ kind: "text", text: content }],
            },
          ],
        };
      }

      // Araç sonuçları: ilgili tool part'a iliştir.
      if (Array.isArray(content)) {
        let messages = state.messages;
        for (const block of asArray(content)) {
          if (block.type !== "tool_result") continue;

          const isError = block.is_error === true;
          messages = patchTool(messages, String(block.tool_use_id), {
            state: isError ? "output-error" : "output-available",
            output: isError ? undefined : block.content,
            errorText: isError ? stringify(block.content) : undefined,
          });
        }
        return { ...state, messages };
      }

      return state;
    }

    case "control_request": {
      const request = event.request as Json | undefined;
      if (request?.subtype !== "can_use_tool") return state;

      const toolCallId = String(request.tool_use_id ?? "");
      return {
        ...state,
        messages: patchTool(state.messages, toolCallId, {
          state: "approval-requested",
          permissionRequestId: String(event.request_id ?? ""),
          suggestions: (request.permission_suggestions as PermissionSuggestion[]) ?? [],
          description: request.description as string | undefined,
        }),
      };
    }

    case "result": {
      return {
        ...state,
        busy: false,
        streamingText: "",
        totalCostUsd:
          typeof event.total_cost_usd === "number"
            ? event.total_cost_usd
            : state.totalCostUsd,
      };
    }

    default:
      return state;
  }
}

export function stringify(value: unknown): string {
  if (typeof value === "string") return value;
  if (value === null || value === undefined) return "";
  try {
    return JSON.stringify(value, null, 2);
  } catch {
    return String(value);
  }
}

/**
 * Diskten okunan transcript kayıtlarını duruma yükler.
 *
 * `claude --resume` geçmişi tekrar yayınlamıyor, bu yüzden arayüzü buradan
 * dolduruyoruz. Kayıtlar canlı akışla aynı şekle sahip olduğu için aynı
 * reducer'dan geçiyorlar — tek kod yolu.
 */
export function seedFromTranscript(records: Json[]): SessionState {
  const seeded = records.reduce(reduce, initialState);

  // Geçmiş yüklemek bir tur başlatmaz; ayrıca yarım kalmış izin istekleri
  // artık cevaplanamaz (o kontrol kanalı kapandı).
  return {
    ...seeded,
    busy: false,
    streamingText: "",
    messages: seeded.messages.map((message) => ({
      ...message,
      parts: message.parts.map((part) =>
        part.kind === "tool" && part.state === "approval-requested"
          ? { ...part, state: "output-denied" as const, permissionRequestId: undefined }
          : part,
      ),
    })),
  };
}

/** Kullanıcı mesajını iyimser olarak ekler (Claude onu geri yansıtmıyor). */
export function appendUserMessage(state: SessionState, text: string): SessionState {
  return {
    ...state,
    busy: true,
    messages: [
      ...state.messages,
      { id: nextId("user"), role: "user", parts: [{ kind: "text", text }] },
    ],
  };
}
