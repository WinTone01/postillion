import { invoke } from "@tauri-apps/api/core";

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
  }) =>
    invoke<string>("agent_start", {
      id: args.id,
      cwd: args.cwd ?? null,
      resume: args.resume ?? null,
      model: args.model ?? null,
      effort: args.effort ?? null,
    }),

  agentSend: (id: string, text: string) => invoke<void>("agent_send", { id, text }),

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
  const diff = Date.now() - ms;
  const min = Math.floor(diff / 60000);
  if (min < 1) return "az önce";
  if (min < 60) return `${min} dk önce`;
  const hour = Math.floor(min / 60);
  if (hour < 24) return `${hour} sa önce`;
  const day = Math.floor(hour / 24);
  if (day < 30) return `${day} gün önce`;
  return new Date(ms).toLocaleDateString("tr-TR");
}

/** `/home/kullanici/Projects/foo` → `~/Projects/foo` */
export function prettyCwd(cwd: string | null): string {
  if (!cwd) return "bilinmiyor";
  return cwd.replace(/^\/home\/[^/]+/, "~");
}
