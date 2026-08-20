import { invoke } from "@tauri-apps/api/core";

import { formatRelative, t } from "@/lib/i18n";

/** Kullanıcı mesajına iliştirilen görüntü; Rust tarafındaki `agent::Image`. */
export interface ImageAttachment {
  mediaType: string;
  /** Base64 — ham ikili veri IPC'den geçemiyor. */
  data: string;
}

/** Oturumun `claude` sürecinin altında çalışan bir süreç. */
export interface Proc {
  pid: number;
  ppid: number;
  command: string;
  /** `stat` alanındaki tek harf: R çalışıyor, S uyuyor, Z zombi… */
  state: string;
  elapsedSecs: number;
}

/** `/usage` çıktısındaki tek bir limit penceresi. */
export interface UsageWindow {
  /** İngilizce etiket: "session", "week (all models)", "week (Opus)"… */
  label: string;
  percent: number;
  /** Sıfır kullanımda gelmiyor. */
  resets: string | null;
}

export interface Usage {
  windows: UsageWindow[];
  measuredAtMs: number;
  /** Komutun tam çıktısı; ayrıntı kartında gösteriliyor. */
  detail: string;
}

/** Oturum başına kalıcı tercihler; `sessionId` ile anahtarlanıyor. */
export interface SessionPrefs {
  /** Seçilen MCP sunucuları; alan yoksa genel yapılandırma. */
  mcpServers?: string[];
}

/** Rust tarafındaki `accounts::Account` ile birebir. */
export interface Account {
  /** Dizin adı; komutlarda kimlik olarak bu kullanılıyor. */
  slug: string;
  /** Arayüzde görünen ad. */
  label: string;
  email: string | null;
  displayName: string | null;
  organizationRole: string | null;
  seatTier: string | null;
  /** Sistem genelinde etkin hesap bu mu. */
  isActive: boolean;
  hasCredentials: boolean;
}

/** Rust tarafındaki `sessions::Session` ile birebir. */
export interface Session {
  sessionId: string;
  path: string;
  cwd: string | null;
  gitBranch: string | null;
  title: string | null;
  lastPrompt: string | null;
  model: string | null;
  sizeBytes: number;
  modifiedMs: number;
}

export const api = {
  listAccounts: () => invoke<Account[]>("list_accounts"),
  /** Sistem genelinde etkin hesabı değiştirir; terminaldeki `claude` de etkilenir. */
  switchAccount: (slug: string) => invoke<Account>("switch_account", { slug }),
  removeAccount: (slug: string) => invoke<void>("remove_account", { slug }),

  /** Giriş akışını başlatır; URL `auth://url` eventiyle gelir. */
  loginStart: (email?: string) => invoke<void>("login_start", { email: email ?? null }),
  loginSubmitCode: (code: string) => invoke<void>("login_submit_code", { code }),
  loginCancel: () => invoke<void>("login_cancel"),

  listSessions: (project?: string) =>
    invoke<Session[]>("list_sessions", { project: project ?? null }),
  refreshSessions: () => invoke<Session[]>("refresh_sessions"),

  /**
   * Diskteki mevcut içerik.
   *
   * Onay bekleyen `Write` çağrılarında disk hâlâ "önceki" sürümü tuttuğu için
   * gerçek diff üretmeye yarıyor.
   */
  readTextFile: (path: string) => invoke<FileSnapshot>("read_text_file", { path }),

  /**
   * Bir oturumun geçmişini diskten okur.
   *
   * `claude --resume` geçmişi tekrar yayınlamadığı için sohbet geçmişi
   * buradan geliyor.
   */
  readTranscript: (path: string, maxRecords?: number) =>
    invoke<Array<Record<string, unknown>>>("read_transcript", {
      path,
      maxRecords: maxRecords ?? null,
    }),

  agentStart: (args: {
    id: string;
    cwd?: string | null;
    resume?: string | null;
    model?: string | null;
    effort?: string | null;
    /** `null` genel MCP yapılandırması; liste yalnızca seçilenler. */
    mcpServers?: string[] | null;
  }) =>
    invoke<string>("agent_start", {
      id: args.id,
      cwd: args.cwd ?? null,
      resume: args.resume ?? null,
      model: args.model ?? null,
      effort: args.effort ?? null,
      mcpServers: args.mcpServers ?? null,
    }),

  /** Oturumun alt süreçleri; oturum kapalıysa boş. */
  agentProcesses: (id: string) => invoke<Proc[]>("agent_processes", { id }),

  /** Bir alt süreci durdurur; `force` ile SIGKILL. */
  agentKillProcess: (id: string, pid: number, force = false) =>
    invoke<void>("agent_kill_process", { id, pid, force }),

  agentSend: (id: string, text: string, images: ImageAttachment[] = []) =>
    invoke<void>("agent_send", { id, text, images }),

  /**
   * Bölge seçtirip ekran görüntüsü alır; kullanıcı iptal ederse `null`.
   * Yakalama sırasında uygulama penceresi gizleniyor.
   */
  captureScreenshot: () => invoke<ImageAttachment | null>("capture_screenshot"),

  /** Panodaki görüntü; yoksa `null`. Okuma ve PNG'ye çevirme Rust tarafında. */
  clipboardImage: () => invoke<ImageAttachment | null>("clipboard_image"),

  /** Oturum kimliği → kalıcı tercihler. */
  sessionPrefs: () => invoke<Record<string, SessionPrefs>>("session_prefs"),

  /** Bir sohbetin MCP seçimini kalıcı yapar; `null` genel yapılandırma. */
  setSessionMcp: (sessionId: string, servers: string[] | null) =>
    invoke<void>("set_session_mcp", { sessionId, servers }),

  /** Diskteki son ölçümler: hesap kısa adı → kullanım. */
  usageCache: () => invoke<Record<string, Usage>>("usage_cache"),

  /**
   * Etkin hesabın kullanımını ölçer. Etkin hesap yoksa `null`.
   */
  refreshUsage: () => invoke<Usage | null>("refresh_usage"),

  /**
   * Bütün hesapların kullanımını ölçüp güncel önbelleği döndürür.
   *
   * Resmi API her hesabı kendi saklanmış jetonuyla sorguladığı için ölçmek
   * adına hesap değiştirmek gerekmiyor; etkin olmayan hesaplarda gösterilen
   * değer artık "en son etkin olduğunda" değil, şu anki değer.
   */
  refreshAllUsage: () => invoke<Record<string, Usage>>("refresh_all_usage"),

  /** Modeli süren oturumda değiştirir; sohbet bağlamı korunur. */
  agentSetModel: (id: string, model: string) =>
    invoke<void>("agent_set_model", { id, model }),

  agentRespondPermission: (args: {
    id: string;
    requestId: string;
    allow: boolean;
    updatedInput?: unknown;
    message?: string;
  }) =>
    invoke<void>("agent_respond_permission", {
      id: args.id,
      requestId: args.requestId,
      allow: args.allow,
      updatedInput: args.updatedInput ?? null,
      message: args.message ?? null,
    }),

  /** manual | acceptEdits | plan | auto | dontAsk | bypassPermissions */
  agentSetPermissionMode: (id: string, mode: string) =>
    invoke<void>("agent_set_permission_mode", { id, mode }),

  agentInterrupt: (id: string) => invoke<void>("agent_interrupt", { id }),
  agentStop: (id: string) => invoke<void>("agent_stop", { id }),

  // ---------------------------------------------------------------- katalog

  listModels: () => invoke<ModelOption[]>("list_models"),
  effortLevels: () => invoke<string[]>("effort_levels"),

  readPreferences: () => invoke<Preferences>("read_preferences"),
  writePreferences: (preferences: Preferences) =>
    invoke<void>("write_preferences", { preferences }),

  listMcpServers: () => invoke<McpServer[]>("list_mcp_servers"),
  mcpAdd: (args: {
    name: string;
    transport: "http" | "sse" | "stdio";
    target: string;
    headers: string[];
    env: string[];
    commandArgs: string[];
    projectScope: boolean;
  }) => invoke<void>("mcp_add", args),
  mcpRemove: (name: string) =>
    invoke<void>("mcp_remove", { name }),

  listPlugins: () => invoke<Plugin[]>("list_plugins"),
  listAvailablePlugins: () =>
    invoke<Plugin[]>("list_available_plugins"),
  pluginInstall: (id: string) =>
    invoke<void>("plugin_install", { id }),
  pluginUninstall: (id: string) =>
    invoke<void>("plugin_uninstall", { id }),
  pluginSetEnabled: (id: string, enabled: boolean) =>
    invoke<void>("plugin_set_enabled", { id, enabled }),

  listMarketplaces: () =>
    invoke<Marketplace[]>("list_marketplaces"),
  marketplaceAdd: (source: string) =>
    invoke<void>("marketplace_add", { source }),
  marketplaceRemove: (name: string) =>
    invoke<void>("marketplace_remove", { name }),
  marketplaceUpdate: (name?: string) =>
    invoke<void>("marketplace_update", { name: name ?? null }),

  listSkills: () => invoke<Skill[]>("list_skills"),
  skillCreate: (name: string, description?: string) =>
    invoke<void>("skill_create", { name, description: description ?? null }),
  skillDelete: (name: string) =>
    invoke<void>("skill_delete", { name }),
};

export interface FileSnapshot {
  exists: boolean;
  /** Metin değilse ya da 2 MB'ı aşıyorsa null. */
  content: string | null;
  truncated: boolean;
  sizeBytes: number;
}

export interface ModelOption {
  value: string;
  label: string;
  description: string | null;
}

export interface Preferences {
  model?: string;
  effortLevel?: string;
  theme?: string;
}

export interface McpServer {
  name: string;
  transport: string;
  target: string;
  /** Hangi projeye ait; null ise kullanıcı genelinde. */
  scope: string | null;
  /**
   * Gizli değer taşıyan alanların **isimleri**. Değerler kasıtlı olarak
   * backend'den hiç gönderilmiyor.
   */
  secretFields: string[];
}

export interface Plugin {
  id: string;
  version: string | null;
  scope: string | null;
  enabled: boolean | null;
  description: string | null;
  installPath: string | null;
  /** Yalnızca kurulabilir listede dolu. */
  marketplace: string | null;
  installCount: number | null;
  mcpServerNames: string[];
}

export interface Marketplace {
  name: string;
  source: string | null;
  url: string | null;
  repo: string | null;
  installLocation: string | null;
}

export interface Skill {
  name: string;
  description: string | null;
  /** "user" ya da eklenti kimliği. */
  source: string;
  path: string;
}

/** Tauri komut hataları düz string olarak geliyor (error.rs'teki Serialize). */
export function errText(e: unknown): string {
  if (typeof e === "string") return e;
  if (e instanceof Error) return e.message;
  return String(e);
}

export function formatBytes(n: number): string {
  if (n < 1024) return `${n} B`;
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(0)} KB`;
  return `${(n / (1024 * 1024)).toFixed(1)} MB`;
}

export function formatWhen(ms: number): string {
  return formatRelative(ms);
}

/** `/home/kullanici/Projects/foo` → `~/Projects/foo` */
export function prettyCwd(cwd: string | null): string {
  if (!cwd) return t("bilinmiyor");
  return cwd.replace(/^\/home\/[^/]+/, "~");
}
