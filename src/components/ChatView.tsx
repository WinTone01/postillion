import { useEffect, useMemo, useState } from "react";
import { AlertTriangleIcon, FolderIcon, GitBranchIcon, SquareIcon } from "lucide-react";

import {
  Conversation,
  ConversationContent,
  ConversationEmptyState,
  ConversationScrollButton,
} from "@/components/ai-elements/conversation";
import {
  Confirmation,
  ConfirmationAccepted,
  ConfirmationAction,
  ConfirmationActions,
  ConfirmationRejected,
  ConfirmationRequest,
  ConfirmationTitle,
} from "@/components/ai-elements/confirmation";
import {
  Message,
  MessageContent,
  MessageResponse,
} from "@/components/ai-elements/message";
import {
  PromptInput,
  PromptInputBody,
  PromptInputFooter,
  PromptInputProvider,
  PromptInputSubmit,
  PromptInputTextarea,
  PromptInputTools,
  usePromptInputController,
  type PromptInputMessage,
} from "@/components/ai-elements/prompt-input";
import {
  Reasoning,
  ReasoningContent,
  ReasoningTrigger,
} from "@/components/ai-elements/reasoning";
import {
  Tool,
  ToolContent,
  ToolHeader,
  ToolInput,
  ToolOutput,
} from "@/components/ai-elements/tool";
import DiffView from "@/components/DiffView";
import type { MascotState } from "@/components/Mascot";
import { diffFromToolInput } from "@/lib/diff";
import { Button } from "@/components/ui/button";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { api, prettyCwd, type ModelOption } from "@/api";
import { log } from "@/lib/log";
import { stringify, type SlashCommand, type ToolPart } from "@/lib/claude-stream";
import { useAgentSession, type AgentSessionOptions } from "@/hooks/useAgentSession";

interface Props {
  options: AgentSessionOptions;
  title: string;
  gitBranch: string | null;
  /** Maskotun beslendiği durum; üst bileşen sekmeler arasında topluyor. */
  onStateChange?: (id: string, state: MascotState) => void;
}

/**
 * Bekleyen bir `Write` için diskteki mevcut içeriği taban olarak okur.
 *
 * Zamanlama kritik: yazım henüz gerçekleşmediyse disk hâlâ *önceki* sürümü
 * tutuyor ve gerçek diff çıkarılabiliyor. Yazım tamamlandıysa diskteki içerik
 * artık *sonraki* sürüm olduğundan okumak yanıltıcı olurdu — o yüzden yalnızca
 * bekleyen durumlarda okuyoruz.
 */
function useWriteBaseline(part: ToolPart) {
  const [baseline, setBaseline] = useState<{
    content: string | null;
    exists: boolean;
  } | null>(null);

  const filePath =
    part.input && typeof part.input === "object"
      ? ((part.input as Record<string, unknown>).file_path as string | undefined)
      : undefined;

  const pending =
    part.state === "approval-requested" ||
    part.state === "input-streaming" ||
    part.state === "input-available";

  useEffect(() => {
    if (part.name !== "Write" || !pending || !filePath) return;

    let cancelled = false;
    api
      .readTextFile(filePath)
      .then((snapshot) => {
        if (cancelled) return;
        // İkili ya da çok büyük dosyada içerik yok; taban kurulamaz.
        if (snapshot.exists && snapshot.content === null) return;
        setBaseline({ content: snapshot.content, exists: snapshot.exists });
      })
      .catch((e) => log("error", "taban dosya okunamadı:", e));

    return () => {
      cancelled = true;
    };
  }, [part.name, pending, filePath]);

  return baseline;
}

/** Kullanıcının bir izin isteğine verdiği cevabı yukarı taşır. */
type RespondFn = ReturnType<typeof useAgentSession>["respondPermission"];

function ToolBlock({ part, respond }: { part: ToolPart; respond: RespondFn }) {
  // CLI "hep izin ver" kısayolunu öneriyorsa onu da butona çeviriyoruz.
  const setModeSuggestion = part.suggestions?.find((s) => s.type === "setMode");

  // İzin kutusu yalnızca gerçekten bir izin etkileşimi olduysa görünmeli.
  // Aksi halde geçmişteki her araç çağrısının altında anlamsız bir
  // "… çalıştırılsın mı?" kutusu çıkıyor.
  const hasApprovalFlow =
    part.state === "approval-requested" || part.approved !== undefined;

  const approval = useMemo(() => {
    if (part.approved === undefined) return { id: part.toolCallId };
    return { id: part.toolCallId, approved: part.approved };
  }, [part.toolCallId, part.approved]);

  const baseline = useWriteBaseline(part);

  // Edit/Write çağrılarında ham JSON okunmaz; diff olarak gösteriyoruz.
  const change = useMemo(
    () => diffFromToolInput(part.name, part.input, baseline),
    [part.name, part.input, baseline],
  );

  return (
    <div className="w-full">
      {change ? (
        <DiffView
          baselineUnknown={change.baselineUnknown}
          diff={change.diff}
          filePath={change.filePath}
          isNewFile={change.isNewFile}
          pending={part.state === "input-available" || part.state === "input-streaming"}
        />
      ) : (
        <Tool defaultOpen={part.state === "approval-requested"}>
          <ToolHeader type={`tool-${part.name}`} state={part.state} />
          <ToolContent>
            <ToolInput input={part.input} />
            <ToolOutput output={part.output} errorText={part.errorText} />
          </ToolContent>
        </Tool>
      )}

      {hasApprovalFlow && (
      <Confirmation approval={approval} state={part.state} className="mb-4">
        <ConfirmationTitle>
          <span className="font-medium">{part.name}</span>
          {part.description ? ` — ${part.description}` : ""} çalıştırılsın mı?
        </ConfirmationTitle>

        <ConfirmationRequest>
          <ConfirmationActions>
            <ConfirmationAction
              variant="ghost"
              onClick={() =>
                void respond({
                  toolCallId: part.toolCallId,
                  requestId: part.permissionRequestId ?? "",
                  allow: false,
                })
              }
            >
              Reddet
            </ConfirmationAction>

            {setModeSuggestion?.mode && (
              <ConfirmationAction
                variant="secondary"
                onClick={() =>
                  void respond({
                    toolCallId: part.toolCallId,
                    requestId: part.permissionRequestId ?? "",
                    allow: true,
                    setMode: setModeSuggestion.mode,
                  })
                }
              >
                Hep izin ver
              </ConfirmationAction>
            )}

            <ConfirmationAction
              onClick={() =>
                void respond({
                  toolCallId: part.toolCallId,
                  requestId: part.permissionRequestId ?? "",
                  allow: true,
                })
              }
            >
              İzin ver
            </ConfirmationAction>
          </ConfirmationActions>
        </ConfirmationRequest>

        <ConfirmationAccepted>
          <span className="text-muted-foreground text-xs">İzin verildi.</span>
        </ConfirmationAccepted>
        <ConfirmationRejected>
          <span className="text-muted-foreground text-xs">Reddedildi.</span>
        </ConfirmationRejected>
      </Confirmation>
      )}
    </div>
  );
}

/**
 * Slash komutu otomatik tamamlama.
 *
 * Komut listesi yalnızca `initialize` handshake'inden geliyor — CLI bunu başka
 * hiçbir yerden sunmuyor. Girdi "/" ile başlayıp henüz boşluk içermiyorsa
 * filtrelenmiş liste açılıyor.
 */
function SlashPalette({ commands }: { commands: SlashCommand[] }) {
  const { textInput } = usePromptInputController();
  const value = textInput.value;

  const matches = useMemo(() => {
    if (!value.startsWith("/") || value.includes(" ") || value.includes("\n")) {
      return [];
    }
    const query = value.slice(1).toLocaleLowerCase("tr");
    return commands
      .filter((c) => c.name.toLocaleLowerCase("tr").includes(query))
      .slice(0, 8);
  }, [value, commands]);

  if (matches.length === 0) return null;

  return (
    <div className="mb-2 max-h-64 overflow-y-auto rounded-lg border bg-popover shadow-md">
      {matches.map((command) => (
        <button
          className="flex w-full flex-col items-start gap-0.5 px-3 py-2 text-left hover:bg-accent"
          key={command.name}
          onClick={() => textInput.setInput(`/${command.name} `)}
          type="button"
        >
          <span className="font-medium text-sm">/{command.name}</span>
          {command.description && (
            <span className="line-clamp-1 text-muted-foreground text-xs">
              {command.description}
            </span>
          )}
        </button>
      ))}
    </div>
  );
}

/** `/effort` bir slash komutu; süren oturumda da çalışıyor (ölçüldü). */
const EFFORT_LEVELS = ["low", "medium", "high", "xhigh", "max"];

export default function ChatView({ options, title, gitBranch, onStateChange }: Props) {
  const { state, running, loadingHistory, send, respondPermission, interrupt } =
    useAgentSession(options);

  const [fallbackModels, setFallbackModels] = useState<ModelOption[]>([]);
  // Efor süren oturumda `/effort` ile değişiyor; başlangıç değeri seçenekten.
  const [effort, setEffort] = useState(options.effort ?? "");
  // Sunucudan gelen model adı (`system/init`) tam ad; seçici takma adla
  // çalışıyor, o yüzden seçimi ayrı tutuyoruz.
  const [picked, setPicked] = useState(options.model ?? "");

  useEffect(() => {
    api
      .listModels(options.account)
      .then(setFallbackModels)
      .catch((e) => log("error", "modeller alınamadı:", e));
  }, [options.account]);

  // Handshake'ten gelen liste daha doğru (hesabın gerçekten erişebildikleri);
  // gelene kadar katalogdaki listeyi gösteriyoruz.
  const models =
    state.models.length > 0
      ? state.models.map((m) => ({
          value: m.value,
          label: m.displayName,
          description: m.description ?? null,
        }))
      : fallbackModels;

  /** Efor için ayrı bir kontrol isteği yok; slash komutu olarak gönderiliyor. */
  async function changeEffort(level: string) {
    setEffort(level);
    await send(`/effort ${level}`);
  }

  async function changeModel(value: string) {
    setPicked(value);
    try {
      // Süreç yeniden başlatılmıyor; sohbet bağlamı korunuyor.
      await api.agentSetModel(options.id, value);
    } catch (e) {
      log("error", "model değiştirilemedi:", e);
    }
  }

  // Oturumun durumunu maskot için tek bir değere indirger.
  // Sıralama önem taşıyor: en çok ilgi isteyen durum kazanır.
  const mascotState: MascotState = useMemo(() => {
    const tools = state.messages.flatMap((m) =>
      m.parts.filter((p): p is ToolPart => p.kind === "tool"),
    );

    if (state.errors.length > 0) return "error";
    if (tools.some((t) => t.state === "approval-requested")) return "waiting";
    if (tools.some((t) => t.state === "input-available")) return "working";
    if (state.busy) return "thinking";
    return "idle";
  }, [state.messages, state.errors.length, state.busy]);

  useEffect(() => {
    onStateChange?.(options.id, mascotState);
  }, [onStateChange, options.id, mascotState]);

  async function handleSubmit(message: PromptInputMessage) {
    const text = message.text?.trim();
    if (!text) return;
    await send(text);
  }

  return (
    <div className="flex h-full flex-col">
      <header className="flex items-center gap-3 border-b bg-card/40 px-5 py-2.5 backdrop-blur">
        <div className="min-w-0 flex-1">
          <div className="flex items-center gap-2">
            <h2 className="truncate font-medium text-sm">{title}</h2>
            {state.busy && (
              <span className="inline-flex items-center gap-1.5 rounded-full bg-primary/10 px-2 py-0.5 text-[10px] text-primary">
                <span className="size-1.5 animate-pulse rounded-full bg-primary" />
                çalışıyor
              </span>
            )}
          </div>
          <div className="mt-0.5 flex items-center gap-2.5 text-[11px] text-muted-foreground">
            <span className="inline-flex min-w-0 items-center gap-1">
              <FolderIcon className="size-3 shrink-0" />
              <span className="truncate">{prettyCwd(options.cwd)}</span>
            </span>
            {gitBranch && gitBranch !== "HEAD" && (
              <span className="inline-flex items-center gap-1">
                <GitBranchIcon className="size-3" />
                {gitBranch}
              </span>
            )}
            <span className="opacity-40">·</span>
            <span>{options.account}</span>
            {state.totalCostUsd !== null && (
              <>
                <span className="opacity-40">·</span>
                <span className="tabular-nums">
                  ${state.totalCostUsd.toFixed(4)}
                </span>
              </>
            )}
          </div>
        </div>

        <Select disabled={!running} onValueChange={changeModel} value={picked}>
          <SelectTrigger className="h-8 w-[140px] text-xs">
            <SelectValue placeholder={state.model ?? "Model"} />
          </SelectTrigger>
          <SelectContent>
            {models.map((model) => (
              <SelectItem key={model.value} value={model.value}>
                {model.label}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>

        <Select
          disabled={!running || state.busy}
          onValueChange={(v) => void changeEffort(v)}
          value={effort}
        >
          <SelectTrigger className="h-8 w-[110px] text-xs">
            <SelectValue placeholder="Efor" />
          </SelectTrigger>
          <SelectContent>
            {EFFORT_LEVELS.map((level) => (
              <SelectItem key={level} value={level}>
                {level}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>

        {state.busy && (
          <Button size="sm" variant="ghost" onClick={() => void interrupt()}>
            <SquareIcon className="size-3" />
            Durdur
          </Button>
        )}
      </header>

      {state.errors.length > 0 && (
        <div className="flex items-start gap-2 border-b bg-destructive/10 px-5 py-2">
          <AlertTriangleIcon className="mt-0.5 size-4 shrink-0 text-destructive" />
          <pre className="flex-1 overflow-x-auto whitespace-pre-wrap text-xs">
            {state.errors.slice(-3).join("\n")}
          </pre>
        </div>
      )}

      <Conversation className="min-h-0 flex-1">
        <ConversationContent>
          {state.messages.length === 0 && !state.streamingText && (
            <ConversationEmptyState
              title={
                loadingHistory ? "Geçmiş yükleniyor…" : running ? "Hazır" : "Başlatılıyor…"
              }
              description={
                options.resume
                  ? "Önceki oturum geri yükleniyor."
                  : "Bir şey yazarak başlayın."
              }
            />
          )}

          {state.messages.map((message) => (
            <Message from={message.role} key={message.id}>
              <MessageContent>
                {message.parts.map((part, index) => {
                  if (part.kind === "text") {
                    return <MessageResponse key={index}>{part.text}</MessageResponse>;
                  }
                  if (part.kind === "thinking") {
                    return (
                      <Reasoning className="mb-3" key={index}>
                        <ReasoningTrigger />
                        <ReasoningContent>{part.text}</ReasoningContent>
                      </Reasoning>
                    );
                  }
                  return (
                    <ToolBlock
                      key={part.toolCallId}
                      part={part}
                      respond={respondPermission}
                    />
                  );
                })}
              </MessageContent>
            </Message>
          ))}

          {/* Akmakta olan metin; `assistant` event'i gelince kalıcıya döner. */}
          {state.streamingText && (
            <Message from="assistant">
              <MessageContent>
                <div className="cs-caret">
                  <MessageResponse>{state.streamingText}</MessageResponse>
                </div>
              </MessageContent>
            </Message>
          )}
        </ConversationContent>
        <ConversationScrollButton />
      </Conversation>

      <div className="border-t p-3">
        <PromptInputProvider>
          <SlashPalette commands={state.commands} />
          <PromptInput onSubmit={handleSubmit}>
            <PromptInputBody>
              <PromptInputTextarea
                placeholder={
                  running ? "Claude'a yazın — komutlar için / yazın" : "Oturum başlatılıyor…"
                }
                disabled={!running}
              />
            </PromptInputBody>
            <PromptInputFooter>
              <PromptInputTools />
              <PromptInputSubmit
                status={state.busy ? "streaming" : "ready"}
                onStop={() => void interrupt()}
              />
            </PromptInputFooter>
          </PromptInput>
        </PromptInputProvider>
      </div>
    </div>
  );
}

export { stringify };
