import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  AlertTriangleIcon,
  CameraIcon,
  CpuIcon,
  FolderIcon,
  GaugeIcon,
  GitBranchIcon,
  Loader2Icon,
  PaperclipIcon,
  ShieldIcon,
  SquareIcon,
  XIcon,
} from "lucide-react";
import { toast } from "sonner";

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
  PromptInputButton,
  PromptInputFooter,
  PromptInputProvider,
  PromptInputSubmit,
  PromptInputTextarea,
  PromptInputTools,
  usePromptInputAttachments,
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
import QuestionCard, { formatAnswers, type Question } from "@/components/QuestionCard";
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
import { api, errText, prettyCwd, type ModelOption } from "@/api";
import { attachmentToFile, urlToAttachment } from "@/lib/images";
import { log } from "@/lib/log";
import {
  stringify,
  type SessionState,
  type SlashCommand,
  type ToolPart,
} from "@/lib/claude-stream";
import { useAgentSession, type AgentSessionOptions } from "@/hooks/useAgentSession";
import { useTypewriter } from "@/hooks/useTypewriter";
import { fireAlert, loadAlertSettings } from "@/lib/alerts";

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

/**
 * İzin bloğunun DOM kimliği.
 *
 * Sekme kimliğiyle önekleniyor: açık sekmelerin hepsi mount kalıyor ve
 * yalnızca araç kimliği kullanılsaydı iki sekmede aynı id bulunurdu.
 */
function anchorFor(sessionId: string, toolCallId: string): string {
  return `perm-${sessionId}-${toolCallId}`;
}

/** Kullanıcının bir izin isteğine verdiği cevabı yukarı taşır. */
type RespondFn = ReturnType<typeof useAgentSession>["respondPermission"];

type AnswerFn = ReturnType<typeof useAgentSession>["answerQuestions"];

/**
 * `setMode` önerilerinin insan okunur karşılığı.
 *
 * CLI ham mod adını gönderiyor ("acceptEdits"); butonda böyle görünmesi neyin
 * kabul edildiğini gizliyordu.
 */
const MODE_LABELS: Record<string, string> = {
  acceptEdits: "Düzenlemeleri hep onayla",
  bypassPermissions: "İzin sormayı kapat",
  plan: "Plan moduna geç",
  default: "Varsayılan moda dön",
};

function ToolBlock({
  part,
  respond,
  answer,
  anchorId,
  onAllowTool,
}: {
  part: ToolPart;
  respond: RespondFn;
  answer: AnswerFn;
  /** Bekleyen izin çubuğundan bu bloğa atlayabilmek için. */
  anchorId: string;
  /** Bu araca oturum boyunca izin ver. */
  onAllowTool: (name: string) => void;
}) {
  // AskUserQuestion bir izin isteği gibi geliyor ama aslında bir soru;
  // ham JSON yerine seçim arayüzü çiziyoruz.
  if (part.name === "AskUserQuestion") {
    const questions =
      (part.input as { questions?: Question[] } | undefined)?.questions ?? [];

    if (questions.length > 0) {
      const answered =
        part.answered ??
        (part.state === "output-available" && typeof part.output === "string"
          ? part.output
          : null);

      return (
        <QuestionCard
          answered={answered}
          onSubmit={(answers) =>
            void answer({
              toolCallId: part.toolCallId,
              requestId: part.permissionRequestId ?? "",
              summary: Object.values(answers).join(" · "),
              message: formatAnswers(answers),
            })
          }
          questions={questions}
        />
      );
    }
  }

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
    <div className="w-full" id={anchorId}>
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

            {/* Tek tek onaylamak uzun sürüyor; aynı araç tekrar tekrar
                soruluyorsa oturum boyunca geçilebilmeli. */}
            <ConfirmationAction
              variant="secondary"
              onClick={() => {
                onAllowTool(part.name);
                void respond({
                  toolCallId: part.toolCallId,
                  requestId: part.permissionRequestId ?? "",
                  allow: true,
                });
              }}
            >
              {part.name}'a hep izin ver
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
                {MODE_LABELS[setModeSuggestion.mode] ?? setModeSuggestion.mode}
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

interface Reveal {
  /** Daktilo şu anda çalışıyor mu. */
  active: boolean;
  /** Hedef metnin ekranda görünen kısmı. */
  shown: string;
  /** Daktilonun sahiplendiği kalıcı mesaj — henüz yoksa null. */
  messageId: string | null;
  /** O mesajdaki parça sırası. */
  partIndex: number;
}

/**
 * Turun sonundaki metni tek bir daktilo akışı olarak yönetir.
 *
 * Neden tek yerde: metin önce `streamingText` önizlemesi olarak, sonra
 * `assistant` event'i gelince kalıcı mesajın parçası olarak çiziliyor. İki ayrı
 * daktilo örneği kullanıldığında bu geçişte metin bir anda tamamlanıyordu —
 * Haiku gibi hızlı modellerde tur o kadar çabuk bitiyor ki efekt hiç
 * görünmeden "ışınlanma" oluyordu. Aynı hedef tek bir hook'tan geçtiği için
 * geçiş artık görünmüyor.
 */
function useReveal(state: SessionState): Reveal {
  // Turun sonundaki metin: ya akan önizleme ya da son asistan mesajının son
  // metin parçası. Boş dize "daktilo edilecek bir şey yok" demek.
  const target = useMemo(() => {
    if (state.streamingText) {
      return { text: state.streamingText, messageId: null, partIndex: -1 };
    }

    const last = state.messages[state.messages.length - 1];
    if (last?.role !== "assistant") return { text: "", messageId: null, partIndex: -1 };

    // Metnin ardından bir araç çağrısı gelmiş olabilir; sondaki parçaya değil,
    // sondaki *metin* parçasına bakıyoruz.
    for (let i = last.parts.length - 1; i >= 0; i -= 1) {
      const part = last.parts[i];
      if (part.kind === "text") {
        return { text: part.text, messageId: last.id, partIndex: i };
      }
    }
    return { text: "", messageId: null, partIndex: -1 };
  }, [state.streamingText, state.messages]);

  // Daktilo yalnızca canlı akış görüldükten sonra devreye giriyor. Aksi halde
  // geçmişi yüklenen bir oturumda son mesaj yeniden yazılırdı.
  const streamed = useRef(false);
  if (state.streamingText.length > 0) streamed.current = true;

  const shown = useTypewriter(target.text, streamed.current);

  // Tur bitse bile daktilo geride kaldıysa yazmayı sürdürüyor; yoksa son anda
  // yine sıçrardı.
  const active =
    streamed.current &&
    target.text.length > 0 &&
    (state.busy || shown.length < target.text.length);

  return { active, shown, messageId: target.messageId, partIndex: target.partIndex };
}

/**
 * Akmakta olan cevap.
 *
 * Markdown yerine düz metin: yarım kalmış markdown (kapanmamış kod bloğu,
 * yarım tablo) her karede farklı ayrıştığı için metin zıplıyordu. Biçimlendirme
 * tam mesaj gelince zaten uygulanıyor.
 *
 * İmleç metnin hemen ardında kendi elemanı olarak duruyor; blok sarmalayıcının
 * `::after`'ı olduğunda bir alt satıra düşüyordu.
 */
function StreamingText({ text }: { text: string }) {
  return (
    <p className="whitespace-pre-wrap break-words text-sm leading-relaxed">
      {text}
      <span className="cs-caret" />
    </p>
  );
}

/**
 * Bekleyen izinler için girdinin hemen üstünde duran çubuk.
 *
 * İstekler sohbetin içinde, bazen ekranın çok yukarısında çiziliyor; birden
 * fazla araç aynı anda izin istediğinde biri gözden kaçıyor ve tur sessizce
 * bekliyordu. Çubuk kaç tane olduğunu söylüyor, hepsini tek hamlede
 * onaylatıyor ve tek tek olanına götürüyor.
 */
function PendingBar({
  parts,
  onAllowAll,
  onJump,
}: {
  parts: ToolPart[];
  onAllowAll: () => void;
  onJump: (part: ToolPart) => void;
}) {
  if (parts.length === 0) return null;

  const names = [...new Set(parts.map((p) => p.name))];
  // Soruların toplu onayı yok; cevap gerektiriyorlar.
  const bulk = parts.filter((p) => p.name !== "AskUserQuestion").length;

  return (
    <div className="flex items-center gap-2 border-warning/30 border-t bg-warning/10 px-3 py-2">
      <span className="size-2 shrink-0 animate-pulse rounded-full bg-warning" />
      <div className="min-w-0 flex-1">
        <p className="truncate font-medium text-xs">
          {parts.length === 1 ? "Bir izin bekliyor" : `${parts.length} izin bekliyor`}
          <span className="ml-1.5 font-normal text-muted-foreground">
            {names.join(", ")}
          </span>
        </p>
      </div>
      <Button onClick={() => onJump(parts[0])} size="sm" variant="ghost">
        Göster
      </Button>
      {bulk > 1 && (
        <Button onClick={onAllowAll} size="sm">
          {bulk} isteğe izin ver
        </Button>
      )}
    </div>
  );
}

/**
 * Gönderilmeyi bekleyen görüntüler.
 *
 * Küçük önizlemeler: bir ekran görüntüsünün doğru olanı olup olmadığı ancak
 * bakarak anlaşılıyor, dosya adı yetmiyor.
 */
function AttachmentStrip() {
  const attachments = usePromptInputAttachments();
  if (attachments.files.length === 0) return null;

  return (
    <div className="flex flex-wrap gap-2 px-3 pt-3">
      {attachments.files.map((file) => (
        <div className="group relative" key={file.id}>
          <img
            alt={file.filename ?? "ek"}
            className="size-16 rounded-lg border object-cover"
            src={file.url}
          />
          <button
            aria-label="Eki kaldır"
            className="-right-1.5 -top-1.5 absolute grid size-5 place-items-center rounded-full border bg-background text-muted-foreground opacity-0 transition-opacity hover:text-foreground group-hover:opacity-100"
            onClick={() => attachments.remove(file.id)}
            type="button"
          >
            <XIcon className="size-3" />
          </button>
        </div>
      ))}
    </div>
  );
}

/**
 * Görüntü iliştirme ve ekran görüntüsü butonları.
 *
 * Yapıştırma ve sürükle-bırak zaten `PromptInput` içinde; bunlar aynı ek
 * listesini besleyen görünür yollar.
 */
function AttachmentButtons({ disabled }: { disabled: boolean }) {
  const attachments = usePromptInputAttachments();
  const [capturing, setCapturing] = useState(false);

  async function screenshot() {
    setCapturing(true);
    try {
      const shot = await api.captureScreenshot();
      // İptal edildiyse `null` gelir; bu bir hata değil.
      if (!shot) return;
      attachments.add([attachmentToFile(shot, `ekran-${Date.now()}.png`)]);
    } catch (e) {
      log("error", "ekran görüntüsü alınamadı:", e);
      toast.error(errText(e));
    } finally {
      setCapturing(false);
    }
  }

  return (
    <>
      <PromptInputButton
        aria-label="Görüntü ekle"
        disabled={disabled}
        onClick={() => attachments.openFileDialog()}
        variant="ghost"
      >
        <PaperclipIcon className="size-3.5" />
      </PromptInputButton>
      <PromptInputButton
        aria-label="Ekran görüntüsü al"
        disabled={disabled || capturing}
        onClick={() => void screenshot()}
        variant="ghost"
      >
        {capturing ? (
          <Loader2Icon className="size-3.5 animate-spin" />
        ) : (
          <CameraIcon className="size-3.5" />
        )}
      </PromptInputButton>
    </>
  );
}

/** `/effort` bir slash komutu; süren oturumda da çalışıyor (ölçüldü). */
const EFFORT_LEVELS = ["low", "medium", "high", "xhigh", "max"];

/**
 * `--permission-mode` seçenekleri (CLI yardımından doğrulandı).
 *
 * `set_permission_mode` kontrol isteğiyle süren oturumda da değişiyor.
 */
const PERMISSION_MODES = [
  { value: "manual", label: "Her şeyi sor" },
  { value: "acceptEdits", label: "Düzenlemeleri onayla" },
  { value: "plan", label: "Plan" },
  { value: "auto", label: "Otomatik" },
  { value: "dontAsk", label: "Sorma" },
  { value: "bypassPermissions", label: "İzinsiz" },
];

/** Alt bardaki kompakt seçici; üçü de aynı görünsün diye ortak. */
function BottomSelect({
  value,
  onChange,
  options,
  placeholder,
  icon,
  disabled,
  width,
}: {
  value: string;
  onChange: (value: string) => void;
  options: { value: string; label: string }[];
  placeholder: string;
  icon: React.ReactNode;
  disabled?: boolean;
  width: string;
}) {
  return (
    <Select disabled={disabled} onValueChange={onChange} value={value}>
      <SelectTrigger
        className={`h-7 gap-1.5 border-none bg-transparent px-2 text-muted-foreground text-xs shadow-none hover:bg-accent/60 ${width}`}
      >
        {icon}
        <SelectValue placeholder={placeholder} />
      </SelectTrigger>
      <SelectContent>
        {options.map((option) => (
          <SelectItem key={option.value} value={option.value}>
            {option.label}
          </SelectItem>
        ))}
      </SelectContent>
    </Select>
  );
}

export default function ChatView({ options, title, gitBranch, onStateChange }: Props) {
  const {
    state,
    running,
    loadingHistory,
    send,
    respondPermission,
    answerQuestions,
    setPermissionMode,
    interrupt,
  } = useAgentSession(options);

  // Mod başlatırken `manual`; süren oturumda set_permission_mode ile değişiyor.
  const [mode, setMode] = useState("manual");

  async function changeMode(next: string) {
    setMode(next);
    await setPermissionMode(next);
  }

  const [fallbackModels, setFallbackModels] = useState<ModelOption[]>([]);
  // Efor süren oturumda `/effort` ile değişiyor; başlangıç değeri seçenekten.
  const [effort, setEffort] = useState(options.effort ?? "");
  // Sunucudan gelen model adı (`system/init`) tam ad; seçici takma adla
  // çalışıyor, o yüzden seçimi ayrı tutuyoruz.
  const [picked, setPicked] = useState(options.model ?? "");

  useEffect(() => {
    api
      .listModels()
      .then(setFallbackModels)
      .catch((e) => log("error", "modeller alınamadı:", e));
  }, []);

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

  /** Cevap bekleyen izin istekleri. */
  const pending = useMemo(
    () =>
      state.messages
        .flatMap((m) => m.parts)
        .filter((p): p is ToolPart => p.kind === "tool" && p.state === "approval-requested"),
    [state.messages],
  );

  /**
   * Oturum boyunca otomatik onaylanan araçlar.
   *
   * Aynı aracın her çağrısını tek tek onaylamak turu uzatıyordu; kullanıcı bir
   * kez "hep izin ver" dediyse sonraki istekler beklemeden geçiyor. Kapsam
   * kasıtlı olarak bu sekme: kalıcı bir izin listesi izin diyaloğunun anlamını
   * sessizce yok ederdi.
   */
  const autoAllowed = useRef(new Set<string>());
  const handled = useRef(new Set<string>());

  const allowTool = useCallback((name: string) => {
    autoAllowed.current.add(name);
  }, []);

  const allowAll = useCallback(() => {
    for (const part of pending) {
      // Soruya düz "allow" cevabı vermek CLI'a "kullanıcı cevaplamadı"
      // dedirtiyor (ölçüldü); soru kartı elle cevaplanmalı.
      if (!part.permissionRequestId || part.name === "AskUserQuestion") continue;
      handled.current.add(part.permissionRequestId);
      void respondPermission({
        toolCallId: part.toolCallId,
        requestId: part.permissionRequestId,
        allow: true,
      });
    }
  }, [pending, respondPermission]);

  function jumpTo(part: ToolPart) {
    document
      .getElementById(anchorFor(options.id, part.toolCallId))
      ?.scrollIntoView({ behavior: "smooth", block: "center" });
  }

  // Otomatik onay ve uyarı, istek BAŞINA veriliyor. Önceden ikisi de maskot
  // durumunun geçişine bağlıydı: durum zaten "waiting" iken gelen ikinci istek
  // ne ses çıkarıyor ne bildirim gönderiyordu, kullanıcı da beklendiğini
  // fark etmiyordu.
  useEffect(() => {
    for (const part of pending) {
      const requestId = part.permissionRequestId;
      if (!requestId || handled.current.has(requestId)) continue;
      handled.current.add(requestId);

      if (autoAllowed.current.has(part.name)) {
        void respondPermission({ toolCallId: part.toolCallId, requestId, allow: true });
        continue;
      }

      const asking = part.name === "AskUserQuestion";
      fireAlert(loadAlertSettings(), asking ? "question" : "permission", {
        title,
        body: asking
          ? "Claude size bir soru sordu."
          : `${part.name} çalıştırmak için onay bekliyor.`,
      });
    }
  }, [pending, respondPermission, title]);

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

  // Tamamlanma ve hata uyarıları durum geçişinde kalıyor; onların istek başına
  // bir karşılığı yok.
  const previousState = useRef<MascotState | null>(null);

  useEffect(() => {
    const before = previousState.current;
    previousState.current = mascotState;
    if (before === null || before === mascotState) return;

    if (mascotState === "idle" && (before === "thinking" || before === "working")) {
      fireAlert(loadAlertSettings(), "done", { title, body: "Claude işini bitirdi." });
    } else if (mascotState === "error") {
      fireAlert(loadAlertSettings(), "error", {
        title,
        body: state.errors.at(-1) ?? "Oturumda bir hata oluştu.",
      });
    }
  }, [mascotState, state.errors, title]);

  const reveal = useReveal(state);

  async function handleSubmit(message: PromptInputMessage) {
    const text = message.text?.trim() ?? "";

    // Ekler `blob:` URL'i olarak tutuluyor; gönderim base64 istiyor.
    const images = (
      await Promise.all(
        (message.files ?? []).map((file) =>
          urlToAttachment(file.url, file.mediaType ?? "").catch((e) => {
            log("error", "ek okunamadı:", e);
            return null;
          }),
        ),
      )
    ).filter((image) => image !== null);

    // Yalnızca görüntüden oluşan bir mesaj da geçerli.
    if (!text && images.length === 0) return;
    await send(text, images);
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
                    // Daktilo hâlâ bu parçanın üzerindeyse markdown yerine
                    // açılmakta olan düz metni çiziyoruz.
                    if (
                      reveal.active &&
                      reveal.messageId === message.id &&
                      reveal.partIndex === index
                    ) {
                      return <StreamingText key={index} text={reveal.shown} />;
                    }
                    return <MessageResponse key={index}>{part.text}</MessageResponse>;
                  }
                  if (part.kind === "image") {
                    return (
                      <img
                        alt="İliştirilen görüntü"
                        className="mb-2 max-h-72 w-auto max-w-full rounded-lg border"
                        key={index}
                        src={part.url}
                      />
                    );
                  }
                  if (part.kind === "thinking") {
                    return (
                      <Reasoning className="mb-3" defaultOpen key={index}>
                        <ReasoningTrigger />
                        <ReasoningContent>{part.text}</ReasoningContent>
                      </Reasoning>
                    );
                  }
                  return (
                    <ToolBlock
                      anchorId={anchorFor(options.id, part.toolCallId)}
                      answer={answerQuestions}
                      key={part.toolCallId}
                      onAllowTool={allowTool}
                      part={part}
                      respond={respondPermission}
                    />
                  );
                })}
              </MessageContent>
            </Message>
          ))}

          {/* Akmakta olan düşünme ve metin; `assistant` event'i gelince
              kalıcı mesaja dönüyorlar. */}
          {(state.streamingThinking || state.streamingText) && (
            <Message from="assistant">
              <MessageContent>
                {state.streamingThinking && (
                  <Reasoning className="mb-3" defaultOpen isStreaming>
                    <ReasoningTrigger />
                    <ReasoningContent>{state.streamingThinking}</ReasoningContent>
                  </Reasoning>
                )}
                {state.streamingText && <StreamingText text={reveal.shown} />}
              </MessageContent>
            </Message>
          )}
        </ConversationContent>
        <ConversationScrollButton />
      </Conversation>

      <PendingBar onAllowAll={allowAll} onJump={jumpTo} parts={pending} />

      <div className="border-t p-3">
        <PromptInputProvider>
          <SlashPalette commands={state.commands} />
          {/* `accept` yalnızca Claude'un okuyabildiği biçimleri geçiriyor;
              başka bir dosya yapıştırıldığında sessizce yok sayılmak yerine
              hata mesajı çıkıyor. */}
          {/* `globalDrop` kasıtlı olarak kapalı: sekmeler açık kaldığı için
              belge geneline bağlanan bırakma dinleyicisi görüntüyü her
              sekmenin girdisine birden eklerdi. Sürükleme girdinin üzerine
              yapılıyor. */}
          <PromptInput
            accept="image/png,image/jpeg,image/gif,image/webp"
            multiple
            onError={(error) => toast.error(error.message)}
            onSubmit={handleSubmit}
          >
            <AttachmentStrip />
            <PromptInputBody>
              <PromptInputTextarea
                placeholder={
                  running ? "Claude'a yazın — komutlar için / yazın" : "Oturum başlatılıyor…"
                }
                disabled={!running}
              />
            </PromptInputBody>
            <PromptInputFooter>
              {/* Model, efor ve mod girdinin yanında — Claude Desktop'taki gibi
                  konuşurken erişilebilir olmalı, başlıkta değil. */}
              <PromptInputTools className="gap-1.5">
                <AttachmentButtons disabled={!running} />
                <BottomSelect
                  disabled={!running}
                  icon={<CpuIcon className="size-3.5" />}
                  onChange={changeModel}
                  options={models.map((m) => ({ value: m.value, label: m.label }))}
                  placeholder={state.model ?? "Model"}
                  value={picked}
                  width="w-[124px]"
                />
                <BottomSelect
                  disabled={!running || state.busy}
                  icon={<GaugeIcon className="size-3.5" />}
                  onChange={(v) => void changeEffort(v)}
                  options={EFFORT_LEVELS.map((l) => ({ value: l, label: l }))}
                  placeholder="Efor"
                  value={effort}
                  width="w-[104px]"
                />
                <BottomSelect
                  disabled={!running}
                  icon={<ShieldIcon className="size-3.5" />}
                  onChange={(v) => void changeMode(v)}
                  options={PERMISSION_MODES}
                  placeholder="Mod"
                  value={mode}
                  width="w-[132px]"
                />
              </PromptInputTools>
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
