import { invoke } from "@tauri-apps/api/core";

/** Rust tarafındaki `accounts::Account` ile birebir (serde camelCase). */
export interface Account {
  name: string;
  dir: string;
  /** `~/.claude` — silinemez, paylaşılan verinin kaynağı. */
  isDefault: boolean;
  loggedIn: boolean;
  email: string | null;
  displayName: string | null;
  organizationRole: string | null;
  seatTier: string | null;
  billingType: string | null;
  /** Boş değilse hesap paylaşılan veriden kopmuş demektir. */
  brokenLinks: string[];
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
  createAccount: (name: string) => invoke<Account>("create_account", { name }),
  repairAccount: (name: string) => invoke<Account>("repair_account", { name }),
  deleteAccount: (name: string) => invoke<void>("delete_account", { name }),
  accountLogin: (account: string) => invoke<void>("account_login", { account }),

  listSessions: (project?: string) =>
    invoke<Session[]>("list_sessions", { project: project ?? null }),
  refreshSessions: () => invoke<Session[]>("refresh_sessions"),

  /**
   * Bir oturumun geçmişini diskten okur.
   *
   * `claude --resume` geçmişi tekrar yayınlamadığı için arayüzdeki sohbet
   * geçmişi buradan geliyor.
   */
  /**
   * Diskteki mevcut içerik.
   *
   * Onay bekleyen `Write` çağrılarında disk hâlâ "önceki" sürümü tuttuğu için
   * gerçek diff üretmeye yarıyor.
   */
  readTextFile: (path: string) => invoke<FileSnapshot>("read_text_file", { path }),

  readTranscript: (path: string, maxRecords?: number) =>
    invoke<Array<Record<string, unknown>>>("read_transcript", {
      path,
      maxRecords: maxRecords ?? null,
    }),

  agentStart: (args: {
    id: string;
    account: string;
    cwd?: string | null;
    resume?: string | null;
    model?: string | null;
    effort?: string | null;
  }) =>
    invoke<string>("agent_start", {
      id: args.id,
      account: args.account,
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

  agentSetPermissionMode: (id: string, mode: string) =>
    invoke<void>("agent_set_permission_mode", { id, mode }),

  agentInterrupt: (id: string) => invoke<void>("agent_interrupt", { id }),
  agentStop: (id: string) => invoke<void>("agent_stop", { id }),

  // ---------------------------------------------------------------- katalog

  listModels: (account: string) => invoke<ModelOption[]>("list_models", { account }),
  effortLevels: () => invoke<string[]>("effort_levels"),

  readPreferences: (account: string) => invoke<Preferences>("read_preferences", { account }),
  writePreferences: (account: string, preferences: Preferences) =>
    invoke<void>("write_preferences", { account, preferences }),

  listMcpServers: (account: string) => invoke<McpServer[]>("list_mcp_servers", { account }),
  mcpAdd: (args: {
    account: string;
    name: string;
    transport: "http" | "sse" | "stdio";
    target: string;
    headers: string[];
    env: string[];
    commandArgs: string[];
    projectScope: boolean;
  }) => invoke<void>("mcp_add", args),
  mcpRemove: (account: string, name: string) =>
    invoke<void>("mcp_remove", { account, name }),

  listPlugins: (account: string) => invoke<Plugin[]>("list_plugins", { account }),
  listAvailablePlugins: (account: string) =>
    invoke<Plugin[]>("list_available_plugins", { account }),
  pluginInstall: (account: string, id: string) =>
    invoke<void>("plugin_install", { account, id }),
  pluginUninstall: (account: string, id: string) =>
    invoke<void>("plugin_uninstall", { account, id }),
  pluginSetEnabled: (account: string, id: string, enabled: boolean) =>
    invoke<void>("plugin_set_enabled", { account, id, enabled }),

  listMarketplaces: (account: string) =>
    invoke<Marketplace[]>("list_marketplaces", { account }),
  marketplaceAdd: (account: string, source: string) =>
    invoke<void>("marketplace_add", { account, source }),
  marketplaceRemove: (account: string, name: string) =>
    invoke<void>("marketplace_remove", { account, name }),
  marketplaceUpdate: (account: string, name?: string) =>
    invoke<void>("marketplace_update", { account, name: name ?? null }),

  listSkills: (account: string) => invoke<Skill[]>("list_skills", { account }),
  skillCreate: (account: string, name: string, description?: string) =>
    invoke<void>("skill_create", { account, name, description: description ?? null }),
  skillDelete: (account: string, name: string) =>
    invoke<void>("skill_delete", { account, name }),
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
