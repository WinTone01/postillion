import { useCallback, useEffect, useMemo, useState } from "react";
import { motion } from "motion/react";
import {
  BellIcon,
  BlocksIcon,
  BrainIcon,
  CheckIcon,
  DownloadIcon,
  KeyRoundIcon,
  Loader2Icon,
  PlugIcon,
  PlusIcon,
  RefreshCwIcon,
  SearchIcon,
  SparklesIcon,
  StoreIcon,
  Trash2Icon,
} from "lucide-react";

import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { Switch } from "@/components/ui/switch";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import { cn } from "@/lib/utils";
import {
  api,
  errText,
  type Marketplace,
  type McpServer,
  type ModelOption,
  type Plugin,
  type Preferences,
  type Skill,
} from "@/api";
import { log } from "@/lib/log";
import {
  ALERT_EVENTS,
  fireAlert,
  loadAlertSettings,
  saveAlertSettings,
  type AlertSettings,
} from "@/lib/alerts";

interface Props {
  onError: (message: string) => void;
}

type Section = "model" | "alerts" | "mcp" | "plugins" | "skills";

const SECTIONS: { id: Section; label: string; icon: React.ReactNode; hint: string }[] = [
  { id: "model", label: "Model & Efor", icon: <BrainIcon className="size-4" />, hint: "Varsayılan model ve düşünme derinliği" },
  { id: "alerts", label: "Bildirimler", icon: <BellIcon className="size-4" />, hint: "Hangi olayda bildirim ve ses" },
  { id: "mcp", label: "MCP", icon: <PlugIcon className="size-4" />, hint: "Sunucular ve erişim anahtarları" },
  { id: "plugins", label: "Eklentiler", icon: <BlocksIcon className="size-4" />, hint: "Marketplace ve kurulu eklentiler" },
  { id: "skills", label: "Skill'ler", icon: <SparklesIcon className="size-4" />, hint: "Sohbette /isim ile çağrılır" },
];

const EFFORT_LABELS: Record<string, string> = {
  low: "Düşük — hızlı ve ucuz",
  medium: "Orta — varsayılan",
  high: "Yüksek",
  xhigh: "Çok yüksek",
  max: "Azami — en yavaş, en derin",
};

export default function SettingsView({ onError }: Props) {
  const [section, setSection] = useState<Section>("model");
  const [models, setModels] = useState<ModelOption[]>([]);
  const [efforts, setEfforts] = useState<string[]>([]);
  const [prefs, setPrefs] = useState<Preferences>({});
  const [mcp, setMcp] = useState<McpServer[]>([]);
  const [plugins, setPlugins] = useState<Plugin[]>([]);
  const [available, setAvailable] = useState<Plugin[] | null>(null);
  const [markets, setMarkets] = useState<Marketplace[]>([]);
  const [skills, setSkills] = useState<Skill[]>([]);
  const [busy, setBusy] = useState(false);
  const [alerts, setAlerts] = useState<AlertSettings>(() => loadAlertSettings());

  const fail = useCallback(
    (context: string, e: unknown) => {
      log("error", context, e);
      onError(`${context}: ${errText(e)}`);
    },
    [onError],
  );

  const loadBasics = useCallback(async () => {
    try {
      const [m, e, p] = await Promise.all([
        api.listModels(),
        api.effortLevels(),
        api.readPreferences(),
      ]);
      setModels(m);
      setEfforts(e);
      setPrefs(p);
    } catch (e) {
      fail("tercihler okunamadı", e);
    }
  }, [fail]);

  const loadMcp = useCallback(async () => {
    try {
      setMcp(await api.listMcpServers());
    } catch (e) {
      fail("MCP sunucuları okunamadı", e);
    }
  }, [fail]);

  const loadPlugins = useCallback(async () => {
    try {
      const [installed, marketList] = await Promise.all([
        api.listPlugins(),
        api.listMarketplaces(),
      ]);
      setPlugins(installed);
      setMarkets(marketList);
    } catch (e) {
      fail("eklentiler okunamadı", e);
    }
  }, [fail]);

  const loadSkills = useCallback(async () => {
    try {
      setSkills(await api.listSkills());
    } catch (e) {
      fail("skill'ler okunamadı", e);
    }
  }, [fail]);

  useEffect(() => {
    void loadBasics();
    void loadMcp();
    void loadPlugins();
    void loadSkills();
  }, [loadBasics, loadMcp, loadPlugins, loadSkills]);

  async function savePrefs(next: Preferences) {
    setPrefs(next);
    try {
      await api.writePreferences(next);
    } catch (e) {
      fail("tercih kaydedilemedi", e);
    }
  }

  /** Ortak sarmalayıcı: meşgul bayrağı, hata yakalama, listeyi tazeleme. */
  async function guarded(context: string, action: () => Promise<void>, reload?: () => void) {
    setBusy(true);
    try {
      await action();
      reload?.();
    } catch (e) {
      fail(context, e);
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="flex h-full">
      {/* Yan gezinme: dört bölüm sekmeye sığmıyordu, dikeyde nefes alıyor. */}
      <nav className="w-[212px] shrink-0 space-y-0.5 border-r bg-sidebar/40 p-3">
        <p className="px-2.5 pb-2 font-medium text-[11px] text-muted-foreground uppercase tracking-wider">
          Ayarlar
        </p>
        {SECTIONS.map((item) => (
          <button
            className={cn(
              "relative flex w-full items-center gap-2.5 rounded-lg px-2.5 py-2 text-left text-sm transition-colors",
              section === item.id
                ? "bg-sidebar-accent font-medium"
                : "text-muted-foreground hover:bg-sidebar-accent/55 hover:text-foreground",
            )}
            key={item.id}
            onClick={() => setSection(item.id)}
            type="button"
          >
            {section === item.id && (
              <motion.span
                className="absolute top-1.5 bottom-1.5 -left-1 w-[3px] rounded-full bg-primary"
                layoutId="settings-marker"
                transition={{ type: "spring", stiffness: 500, damping: 40 }}
              />
            )}
            {item.icon}
            {item.label}
          </button>
        ))}

        <p className="px-2.5 pt-4 text-[11px] text-muted-foreground leading-snug">
          Ayarlar tüm hesaplar için ortak — tek bir yapılandırma var.
        </p>
      </nav>

      <div className="min-w-0 flex-1 overflow-y-auto">
        <div className="mx-auto max-w-3xl px-8 py-7">
          <header className="mb-6">
            <h2 className="font-semibold text-lg">
              {SECTIONS.find((s) => s.id === section)?.label}
            </h2>
            <p className="mt-0.5 text-muted-foreground text-sm">
              {SECTIONS.find((s) => s.id === section)?.hint}
            </p>
          </header>

          {section === "model" && (
            <div className="space-y-5">
              <Field
                hint="Yeni oturumlarda kullanılır. Açık bir sohbetin modelini başlıktaki seçiciden anında değiştirebilirsiniz."
                label="Varsayılan model"
              >
                <Select
                  onValueChange={(value) => void savePrefs({ ...prefs, model: value })}
                  value={prefs.model ?? ""}
                >
                  <SelectTrigger className="w-full">
                    <SelectValue placeholder="Model seçin" />
                  </SelectTrigger>
                  <SelectContent>
                    {models.map((model) => (
                      <SelectItem key={model.value} value={model.value}>
                        {model.label}
                        {model.description ? ` — ${model.description}` : ""}
                      </SelectItem>
                    ))}
                  </SelectContent>
                </Select>
              </Field>

              <Field
                hint="Yeni oturumların başlangıç değeri. Süren bir sohbette başlıktaki efor seçicisi /effort komutunu gönderir."
                label="Efor seviyesi"
              >
                <Select
                  onValueChange={(value) =>
                    void savePrefs({ ...prefs, effortLevel: value })
                  }
                  value={prefs.effortLevel ?? ""}
                >
                  <SelectTrigger className="w-full">
                    <SelectValue placeholder="Seviye seçin" />
                  </SelectTrigger>
                  <SelectContent>
                    {efforts.map((level) => (
                      <SelectItem key={level} value={level}>
                        {EFFORT_LABELS[level] ?? level}
                      </SelectItem>
                    ))}
                  </SelectContent>
                </Select>
              </Field>
            </div>
          )}

          {section === "alerts" && (
            <div className="space-y-4">
            <SectionCard title="Ses düzeyi">
              <div className="flex items-center gap-4 px-4 py-3">
                <input
                  aria-label="Ses düzeyi"
                  className="h-1.5 flex-1 cursor-pointer appearance-none rounded-full bg-muted accent-primary"
                  max={1}
                  min={0}
                  onChange={(e) => {
                    const next = { ...alerts, volume: Number(e.target.value) };
                    setAlerts(next);
                    saveAlertSettings(next);
                  }}
                  step={0.05}
                  type="range"
                  value={alerts.volume}
                />
                <span className="w-10 shrink-0 text-right text-muted-foreground text-xs tabular-nums">
                  {Math.round(alerts.volume * 100)}%
                </span>
                <Button
                  onClick={() =>
                    fireAlert({ ...alerts, done: { notify: false, sound: true } }, "done", {
                      title: "Postillion",
                      body: "Ses örneği",
                    })
                  }
                  size="sm"
                  variant="secondary"
                >
                  Çal
                </Button>
              </div>
            </SectionCard>

            <SectionCard title="Olay başına uyarılar">
              <div className="divide-y">
                {ALERT_EVENTS.map((event) => {
                  const rule = alerts[event.id];
                  const update = (patch: Partial<typeof rule>) => {
                    const next = { ...alerts, [event.id]: { ...rule, ...patch } };
                    setAlerts(next);
                    saveAlertSettings(next);
                  };

                  return (
                    <div className="flex items-center gap-4 px-4 py-3" key={event.id}>
                      <div className="min-w-0 flex-1">
                        <p className="font-medium text-sm">{event.label}</p>
                        <p className="text-[11.5px] text-muted-foreground">{event.hint}</p>
                      </div>

                      <label className="flex shrink-0 items-center gap-2 text-xs">
                        <Switch
                          checked={rule.notify}
                          onCheckedChange={(v) => update({ notify: v })}
                        />
                        Bildirim
                      </label>
                      <label className="flex shrink-0 items-center gap-2 text-xs">
                        <Switch
                          checked={rule.sound}
                          onCheckedChange={(v) => update({ sound: v })}
                        />
                        Ses
                      </label>

                      <Button
                        onClick={() =>
                          fireAlert({ ...alerts, [event.id]: { notify: true, sound: true } }, event.id, {
                            title: "Postillion",
                            body: `${event.label} örneği`,
                          })
                        }
                        size="sm"
                        variant="ghost"
                      >
                        Dene
                      </Button>
                    </div>
                  );
                })}
              </div>
            </SectionCard>
            </div>
          )}

          {section === "mcp" && (
            <McpSection
              busy={busy}
              onAdd={(args) =>
                guarded("MCP sunucusu eklenemedi", () => api.mcpAdd(args), () =>
                  void loadMcp(),
                )
              }
              onRemove={(name) =>
                guarded("MCP sunucusu silinemedi", () => api.mcpRemove(name), () =>
                  void loadMcp(),
                )
              }
              servers={mcp}
            />
          )}

          {section === "plugins" && (
            <PluginSection
              available={available}
              busy={busy}
              guarded={guarded}
              markets={markets}
              onAvailable={setAvailable}
              plugins={plugins}
              reload={() => void loadPlugins()}
            />
          )}

          {section === "skills" && (
            <SkillSection
              busy={busy}
              onCreate={(name, description) =>
                guarded(
                  "skill oluşturulamadı",
                  () => api.skillCreate(name, description),
                  () => void loadSkills(),
                )
              }
              onDelete={(name) =>
                guarded("skill silinemedi", () => api.skillDelete(name), () =>
                  void loadSkills(),
                )
              }
              skills={skills}
            />
          )}
        </div>
      </div>
    </div>
  );
}

/** Etiket + kontrol + açıklama üçlüsü. */
function Field({
  label,
  hint,
  children,
}: {
  label: string;
  hint?: string;
  children: React.ReactNode;
}) {
  return (
    <div className="space-y-1.5">
      <Label className="text-sm">{label}</Label>
      {children}
      {hint && <p className="text-[11.5px] text-muted-foreground leading-snug">{hint}</p>}
    </div>
  );
}

function SectionCard({
  title,
  action,
  children,
}: {
  title: string;
  action?: React.ReactNode;
  children: React.ReactNode;
}) {
  return (
    <section className="rounded-xl border bg-card">
      <div className="flex items-center justify-between gap-2 border-b px-4 py-2.5">
        <h3 className="font-medium text-sm">{title}</h3>
        {action}
      </div>
      {children}
    </section>
  );
}

function EmptyRow({ children }: { children: React.ReactNode }) {
  return <p className="px-4 py-6 text-center text-muted-foreground text-xs">{children}</p>;
}

// ---------------------------------------------------------------------- MCP

interface McpAddArgs {
  name: string;
  transport: "http" | "sse" | "stdio";
  target: string;
  headers: string[];
  env: string[];
  commandArgs: string[];
  projectScope: boolean;
}

function McpSection({
  servers,
  busy,
  onAdd,
  onRemove,
}: {
  servers: McpServer[];
  busy: boolean;
  onAdd: (args: McpAddArgs) => void;
  onRemove: (name: string) => void;
}) {
  const [name, setName] = useState("");
  const [transport, setTransport] = useState<"http" | "sse" | "stdio">("http");
  const [target, setTarget] = useState("");
  const [tokenName, setTokenName] = useState("Authorization");
  const [tokenValue, setTokenValue] = useState("");
  const [extra, setExtra] = useState("");

  const isStdio = transport === "stdio";

  function submit() {
    if (!name.trim() || !target.trim()) return;

    const lines: string[] = [];

    // Ayrı bir "anahtar" alanı: en sık ihtiyaç bu ve serbest metin alanında
    // biçimi yanlış yazmak kolaydı.
    if (tokenValue.trim()) {
      lines.push(
        isStdio
          ? `${tokenName.trim() || "API_KEY"}=${tokenValue.trim()}`
          : `${tokenName.trim() || "Authorization"}: ${tokenValue.trim()}`,
      );
    }
    for (const line of extra.split("\n").map((l) => l.trim()).filter(Boolean)) {
      lines.push(line);
    }

    const [command, ...commandArgs] = target.trim().split(/\s+/);

    onAdd({
      name: name.trim(),
      transport,
      target: isStdio ? command : target.trim(),
      headers: isStdio ? [] : lines,
      env: isStdio ? lines : [],
      commandArgs: isStdio ? commandArgs : [],
      projectScope: false,
    });

    setName("");
    setTarget("");
    setTokenValue("");
    setExtra("");
  }

  return (
    <div className="space-y-5">
      <SectionCard title="Sunucu ekle">
        <div className="space-y-3 p-4">
          <div className="flex gap-2">
            <Input
              className="flex-1"
              onChange={(e) => setName(e.target.value)}
              placeholder="sunucu-adı"
              value={name}
            />
            <Select
              onValueChange={(v) => setTransport(v as typeof transport)}
              value={transport}
            >
              <SelectTrigger className="w-28">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="http">http</SelectItem>
                <SelectItem value="sse">sse</SelectItem>
                <SelectItem value="stdio">stdio</SelectItem>
              </SelectContent>
            </Select>
          </div>

          <Input
            onChange={(e) => setTarget(e.target.value)}
            placeholder={isStdio ? "npx my-mcp-server --flag" : "https://ornek.com/mcp"}
            value={target}
          />

          <div className="rounded-lg border bg-muted/30 p-3">
            <p className="mb-2 flex items-center gap-1.5 font-medium text-xs">
              <KeyRoundIcon className="size-3.5" />
              Erişim anahtarı
              <span className="font-normal text-muted-foreground">(isteğe bağlı)</span>
            </p>

            <div className="flex gap-2">
              <Input
                className="w-[190px] font-mono text-xs"
                onChange={(e) => setTokenName(e.target.value)}
                placeholder={isStdio ? "API_KEY" : "Authorization"}
                value={tokenName}
              />
              <Input
                className="flex-1 font-mono text-xs"
                onChange={(e) => setTokenValue(e.target.value)}
                placeholder={isStdio ? "sk-..." : "Bearer sk-..."}
                type="password"
                value={tokenValue}
              />
            </div>

            <p className="mt-2 text-[11px] text-muted-foreground leading-snug">
              Anahtar doğrudan <code>claude mcp add</code>'e geçer. Bu uygulama onu
              saklamaz ve listede bir daha göstermez — yalnızca alan adını görürsünüz.
            </p>
          </div>

          <details className="group">
            <summary className="cursor-pointer text-muted-foreground text-xs hover:text-foreground">
              Ek {isStdio ? "ortam değişkenleri" : "başlıklar"}
            </summary>
            <textarea
              className="mt-2 min-h-[60px] w-full rounded-lg border bg-transparent px-3 py-2 font-mono text-xs outline-none focus:border-ring"
              onChange={(e) => setExtra(e.target.value)}
              placeholder={isStdio ? "REGION=eu\nDEBUG=1" : "X-Api-Version: 2\nAccept: application/json"}
              value={extra}
            />
          </details>

          <Button disabled={busy || !name.trim() || !target.trim()} onClick={submit} size="sm">
            <PlusIcon className="size-3.5" />
            Sunucu ekle
          </Button>
        </div>
      </SectionCard>

      <SectionCard title={`Yapılandırılmış sunucular (${servers.length})`}>
        <div className="divide-y">
          {servers.length === 0 && <EmptyRow>Henüz MCP sunucusu yok.</EmptyRow>}
          {servers.map((server) => (
            <div
              className="flex items-start gap-3 px-4 py-3"
              key={`${server.scope ?? "user"}/${server.name}`}
            >
              <div className="min-w-0 flex-1">
                <div className="flex items-center gap-2">
                  <p className="truncate font-medium text-sm">{server.name}</p>
                  <span className="rounded bg-muted px-1.5 py-px text-[10px] text-muted-foreground">
                    {server.transport}
                  </span>
                </div>
                <p className="truncate text-[11px] text-muted-foreground">{server.target}</p>
                {server.scope && (
                  <p className="truncate text-[11px] text-muted-foreground">
                    proje: {server.scope}
                  </p>
                )}
                {server.secretFields.length > 0 && (
                  <p className="mt-1 flex items-center gap-1 text-[11px] text-warning">
                    <KeyRoundIcon className="size-3" />
                    {server.secretFields.join(", ")} · gizli
                  </p>
                )}
              </div>

              <IconButton
                busy={busy}
                label="Sunucuyu sil"
                onClick={() => onRemove(server.name)}
              />
            </div>
          ))}
        </div>
      </SectionCard>
    </div>
  );
}

// ----------------------------------------------------------------- eklenti

function PluginSection({
  plugins,
  markets,
  available,
  onAvailable,
  busy,
  guarded,
  reload,
}: {
  plugins: Plugin[];
  markets: Marketplace[];
  available: Plugin[] | null;
  onAvailable: (list: Plugin[]) => void;
  busy: boolean;
  guarded: (c: string, a: () => Promise<void>, r?: () => void) => Promise<void>;
  reload: () => void;
}) {
  const [source, setSource] = useState("");
  const [query, setQuery] = useState("");
  const [loadingList, setLoadingList] = useState(false);

  const installedIds = useMemo(() => new Set(plugins.map((p) => p.id)), [plugins]);

  /**
   * Marketplace'te 2563 eklenti var; hepsini basmak arayüzü kilitler.
   * Arama yoksa en çok kurulan 20 tanesi, arama varsa ilk 40 eşleşme.
   */
  const shown = useMemo(() => {
    if (!available) return [];
    const q = query.trim().toLocaleLowerCase("tr");

    if (!q) {
      return [...available]
        .sort((a, b) => (b.installCount ?? 0) - (a.installCount ?? 0))
        .slice(0, 20);
    }

    return available
      .filter(
        (p) =>
          p.id.toLocaleLowerCase("tr").includes(q) ||
          (p.description ?? "").toLocaleLowerCase("tr").includes(q),
      )
      .slice(0, 40);
  }, [available, query]);

  async function fetchAvailable() {
    setLoadingList(true);
    try {
      onAvailable(await api.listAvailablePlugins());
    } catch (e) {
      log("error", "kurulabilir eklentiler alınamadı:", e);
    } finally {
      setLoadingList(false);
    }
  }

  return (
    <div className="space-y-5">
      <SectionCard
        action={
          <Button
            disabled={busy}
            onClick={() =>
              guarded("marketplace güncellenemedi", () => api.marketplaceUpdate(), reload)
            }
            size="sm"
            variant="ghost"
          >
            <RefreshCwIcon className="size-3.5" />
            Güncelle
          </Button>
        }
        title={`Marketplace'ler (${markets.length})`}
      >
        <div className="flex gap-2 border-b p-4">
          <Input
            onChange={(e) => setSource(e.target.value)}
            onKeyDown={(e) => {
              if (e.key !== "Enter" || !source.trim()) return;
              guarded("marketplace eklenemedi", () => api.marketplaceAdd(source.trim()), reload);
              setSource("");
            }}
            placeholder="kullanıcı/depo, URL ya da yerel yol"
            value={source}
          />
          <Button
            disabled={busy || !source.trim()}
            onClick={() => {
              guarded("marketplace eklenemedi", () => api.marketplaceAdd(source.trim()), reload);
              setSource("");
            }}
            size="sm"
          >
            <PlusIcon className="size-3.5" />
            Ekle
          </Button>
        </div>

        <div className="divide-y">
          {markets.length === 0 && <EmptyRow>Kayıtlı marketplace yok.</EmptyRow>}
          {markets.map((market) => (
            <div className="flex items-center gap-3 px-4 py-2.5" key={market.name}>
              <StoreIcon className="size-4 shrink-0 text-muted-foreground" />
              <div className="min-w-0 flex-1">
                <p className="truncate font-medium text-sm">{market.name}</p>
                <p className="truncate text-[11px] text-muted-foreground">
                  {market.url ?? market.repo ?? market.source ?? ""}
                </p>
              </div>
              <IconButton
                busy={busy}
                label="Marketplace'i kaldır"
                onClick={() =>
                  guarded(
                    "marketplace silinemedi",
                    () => api.marketplaceRemove(market.name),
                    reload,
                  )
                }
              />
            </div>
          ))}
        </div>
      </SectionCard>

      <SectionCard title={`Kurulu eklentiler (${plugins.length})`}>
        <div className="divide-y">
          {plugins.length === 0 && <EmptyRow>Kurulu eklenti yok.</EmptyRow>}
          {plugins.map((plugin) => (
            <div className="flex items-center gap-3 px-4 py-3" key={plugin.id}>
              <div className="min-w-0 flex-1">
                <p className="truncate font-medium text-sm">{plugin.id}</p>
                <p className="truncate text-[11px] text-muted-foreground">
                  {plugin.scope ?? "user"}
                  {plugin.mcpServerNames.length > 0 &&
                    ` · MCP: ${plugin.mcpServerNames.join(", ")}`}
                </p>
              </div>

              <Tooltip>
                <TooltipTrigger asChild>
                  <div>
                    <Switch
                      checked={plugin.enabled ?? false}
                      disabled={busy}
                      onCheckedChange={(checked) =>
                        guarded(
                          "eklenti durumu değiştirilemedi",
                          () => api.pluginSetEnabled(plugin.id, checked),
                          reload,
                        )
                      }
                    />
                  </div>
                </TooltipTrigger>
                <TooltipContent>
                  {plugin.enabled ? "Devre dışı bırak" : "Etkinleştir"}
                </TooltipContent>
              </Tooltip>

              <IconButton
                busy={busy}
                label="Eklentiyi kaldır"
                onClick={() =>
                  guarded(
                    "eklenti kaldırılamadı",
                    () => api.pluginUninstall(plugin.id),
                    reload,
                  )
                }
              />
            </div>
          ))}
        </div>
      </SectionCard>

      <SectionCard
        action={
          available === null ? (
            <Button disabled={loadingList} onClick={fetchAvailable} size="sm" variant="secondary">
              {loadingList ? (
                <Loader2Icon className="size-3.5 animate-spin" />
              ) : (
                <DownloadIcon className="size-3.5" />
              )}
              Listele
            </Button>
          ) : (
            <span className="text-[11px] text-muted-foreground">
              {available.length} eklenti
            </span>
          )
        }
        title="Marketplace'ten kur"
      >
        {available === null ? (
          <EmptyRow>Kurulabilir eklentileri görmek için “Listele”ye basın.</EmptyRow>
        ) : (
          <>
            <div className="relative border-b p-3">
              <SearchIcon className="-translate-y-1/2 pointer-events-none absolute top-1/2 left-6 size-4 text-muted-foreground" />
              <Input
                className="pl-9"
                onChange={(e) => setQuery(e.target.value)}
                placeholder="Eklenti ara…"
                value={query}
              />
            </div>

            <div className="divide-y">
              {!query && (
                <p className="px-4 py-2 text-[11px] text-muted-foreground">
                  En çok kurulan 20 eklenti gösteriliyor — aramayla daraltın.
                </p>
              )}
              {shown.length === 0 && <EmptyRow>Eşleşen eklenti yok.</EmptyRow>}
              {shown.map((plugin) => {
                const installed = installedIds.has(plugin.id);
                return (
                  <div className="flex items-start gap-3 px-4 py-3" key={plugin.id}>
                    <div className="min-w-0 flex-1">
                      <div className="flex items-center gap-2">
                        <p className="truncate font-medium text-sm">{plugin.id}</p>
                        {plugin.installCount != null && (
                          <span className="shrink-0 rounded bg-muted px-1.5 py-px text-[10px] text-muted-foreground tabular-nums">
                            {plugin.installCount.toLocaleString("tr")}
                          </span>
                        )}
                      </div>
                      {plugin.description && (
                        <p className="line-clamp-2 text-[11.5px] text-muted-foreground">
                          {plugin.description}
                        </p>
                      )}
                    </div>

                    {installed ? (
                      <span className="flex shrink-0 items-center gap-1 text-[11px] text-success">
                        <CheckIcon className="size-3.5" />
                        kurulu
                      </span>
                    ) : (
                      <Button
                        disabled={busy}
                        onClick={() =>
                          guarded(
                            "eklenti kurulamadı",
                            () => api.pluginInstall(plugin.id),
                            reload,
                          )
                        }
                        size="sm"
                        variant="secondary"
                      >
                        Kur
                      </Button>
                    )}
                  </div>
                );
              })}
            </div>
          </>
        )}
      </SectionCard>
    </div>
  );
}

// ------------------------------------------------------------------- skill

function SkillSection({
  skills,
  busy,
  onCreate,
  onDelete,
}: {
  skills: Skill[];
  busy: boolean;
  onCreate: (name: string, description?: string) => void;
  onDelete: (name: string) => void;
}) {
  const [name, setName] = useState("");
  const [description, setDescription] = useState("");

  const userSkills = skills.filter((s) => s.source === "user");
  const pluginSkills = skills.filter((s) => s.source !== "user");

  return (
    <div className="space-y-5">
      <SectionCard title="Yeni skill oluştur">
        <div className="space-y-3 p-4">
          <Input
            onChange={(e) => setName(e.target.value)}
            placeholder="skill-adi"
            value={name}
          />
          <Input
            onChange={(e) => setDescription(e.target.value)}
            placeholder="Ne zaman kullanılacağını anlatan kısa açıklama"
            value={description}
          />
          <Button
            disabled={busy || !name.trim()}
            onClick={() => {
              onCreate(name.trim(), description.trim() || undefined);
              setName("");
              setDescription("");
            }}
            size="sm"
          >
            <PlusIcon className="size-3.5" />
            Oluştur
          </Button>
          <p className="text-[11px] text-muted-foreground leading-snug">
            <code>~/.claude/skills/&lt;ad&gt;/</code> altında iskelet oluşturulur ve
            bir sonraki oturumda yüklenir.
          </p>
        </div>
      </SectionCard>

      <SectionCard title={`Kendi skill'leriniz (${userSkills.length})`}>
        <div className="divide-y">
          {userSkills.length === 0 && <EmptyRow>Henüz kendi skill'iniz yok.</EmptyRow>}
          {userSkills.map((skill) => (
            <div className="flex items-start gap-3 px-4 py-3" key={skill.path}>
              <div className="min-w-0 flex-1">
                <p className="font-medium text-sm">/{skill.name}</p>
                {skill.description && (
                  <p className="line-clamp-2 text-[11.5px] text-muted-foreground">
                    {skill.description}
                  </p>
                )}
              </div>
              <IconButton
                busy={busy}
                label="Skill'i sil"
                onClick={() => onDelete(skill.name)}
              />
            </div>
          ))}
        </div>
      </SectionCard>

      <SectionCard title={`Eklentilerden gelenler (${pluginSkills.length})`}>
        <div className="divide-y">
          {pluginSkills.length === 0 && <EmptyRow>Eklenti skill'i yok.</EmptyRow>}
          {pluginSkills.map((skill) => (
            <div className="px-4 py-3" key={skill.path}>
              <div className="flex items-center gap-2">
                <p className="font-medium text-sm">/{skill.name}</p>
                <span className="rounded bg-muted px-1.5 py-px text-[10px] text-muted-foreground">
                  {skill.source}
                </span>
              </div>
              {skill.description && (
                <p className="mt-0.5 line-clamp-2 text-[11.5px] text-muted-foreground">
                  {skill.description}
                </p>
              )}
            </div>
          ))}
        </div>
      </SectionCard>
    </div>
  );
}

function IconButton({
  label,
  onClick,
  busy,
}: {
  label: string;
  onClick: () => void;
  busy: boolean;
}) {
  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <Button
          aria-label={label}
          className="shrink-0 text-muted-foreground hover:text-destructive"
          disabled={busy}
          onClick={onClick}
          size="icon"
          variant="ghost"
        >
          <Trash2Icon className="size-4" />
        </Button>
      </TooltipTrigger>
      <TooltipContent>{label}</TooltipContent>
    </Tooltip>
  );
}
