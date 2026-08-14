import { useEffect, useMemo, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { CheckIcon, FolderIcon, FolderOpenIcon, PlugIcon } from "lucide-react";

import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { cn } from "@/lib/utils";
import { api, formatWhen, prettyCwd, type McpServer, type Session } from "@/api";
import { log } from "@/lib/log";
import { t } from "@/lib/i18n";

interface Props {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  /** Oturum geçmişi; son kullanılan projeleri buradan çıkarıyoruz. */
  sessions: Session[];
  /** `mcpServers` null ise genel MCP yapılandırması kullanılıyor. */
  onStart: (cwd: string, mcpServers: string[] | null) => void;
}

/**
 * Yeni oturumun çalışma dizinini seçtirir.
 *
 * Önceden hiç sorulmuyordu ve Claude uygulamanın kendi çalışma dizininde —
 * pratikte ev dizininde — başlıyordu. Claude Code için çalışma dizini kritik:
 * dosyaları orada arıyor ve transcript'ler ona göre dizinleniyor.
 */
export default function NewSessionDialog({
  open: isOpen,
  onOpenChange,
  sessions,
  onStart,
}: Props) {
  const [path, setPath] = useState("");
  const [servers, setServers] = useState<McpServer[]>([]);
  /** Seçili sunucu adları; `null` "hepsi" demek ve genel yapılandırmayı korur. */
  const [chosen, setChosen] = useState<Set<string> | null>(null);

  /** Geçmiş oturumlardan en son kullanılan dizinler. */
  const recent = useMemo(() => {
    const seen = new Map<string, number>();
    for (const session of sessions) {
      if (!session.cwd) continue;
      const existing = seen.get(session.cwd);
      if (existing === undefined || session.modifiedMs > existing) {
        seen.set(session.cwd, session.modifiedMs);
      }
    }
    return [...seen.entries()]
      .sort((a, b) => b[1] - a[1])
      .slice(0, 6)
      .map(([cwd, modifiedMs]) => ({ cwd, modifiedMs }));
  }, [sessions]);

  useEffect(() => {
    if (!isOpen) return;
    // En son çalışılan proje makul bir varsayılan.
    setPath(recent[0]?.cwd ?? "");
    setChosen(null);
    api
      .listMcpServers()
      .then(setServers)
      .catch((e) => log("warn", "MCP sunucuları okunamadı:", e));
  }, [isOpen, recent]);

  /**
   * Bir sunucuyu açıp kapatır.
   *
   * İlk dokunuşta "hepsi" durumundan çıkılıyor: o ana kadar seçim yok ve genel
   * yapılandırma geçerli.
   */
  function toggleServer(name: string) {
    setChosen((prev) => {
      const next = new Set(prev ?? servers.map((s) => s.name));
      if (next.has(name)) next.delete(name);
      else next.add(name);
      return next;
    });
  }

  async function browse() {
    try {
      const picked = await open({
        directory: true,
        multiple: false,
        title: t("Çalışma dizini seçin"),
        defaultPath: path || undefined,
      });
      if (typeof picked === "string") setPath(picked);
    } catch (e) {
      log("error", "dizin seçilemedi:", e);
    }
  }

  function start() {
    const trimmed = path.trim();
    if (!trimmed) return;

    // Hiçbir şeye dokunulmadıysa genel yapılandırma korunuyor; aksi halde
    // `--strict-mcp-config` devreye girer ve eklenti sunucuları da kapanır.
    const untouched = chosen === null || chosen.size === servers.length;
    onStart(trimmed, untouched ? null : [...chosen]);
    onOpenChange(false);
  }

  return (
    <Dialog onOpenChange={onOpenChange} open={isOpen}>
      <DialogContent className="sm:max-w-lg">
        <DialogHeader>
          <DialogTitle>{t("Yeni oturum")}</DialogTitle>
          <DialogDescription>
            {t(
              "Claude bu dizinde çalışacak — dosyaları burada arar ve oturum buraya kaydedilir.",
            )}
          </DialogDescription>
        </DialogHeader>

        <div className="space-y-4">
          <div className="space-y-1.5">
            <Label className="text-xs">{t("Çalışma dizini")}</Label>
            <div className="flex gap-2">
              <Input
                className="font-mono text-xs"
                onChange={(e) => setPath(e.target.value)}
                onKeyDown={(e) => {
                  if (e.key === "Enter") start();
                }}
                placeholder={t("/home/kullanici/Projects/proje")}
                value={path}
              />
              <Button onClick={() => void browse()} size="icon" variant="secondary">
                <FolderOpenIcon className="size-4" />
              </Button>
            </div>
          </div>

          {recent.length > 0 && (
            <div className="space-y-1.5">
              <Label className="text-xs">{t("Son kullanılanlar")}</Label>
              <div className="space-y-1">
                {recent.map((entry) => (
                  <button
                    className={cn(
                      "flex w-full items-center gap-2 rounded-lg border px-3 py-2 text-left transition-colors",
                      path === entry.cwd
                        ? "border-primary bg-primary/10"
                        : "hover:border-foreground/25 hover:bg-accent/40",
                    )}
                    key={entry.cwd}
                    onClick={() => setPath(entry.cwd)}
                    type="button"
                  >
                    <FolderIcon className="size-3.5 shrink-0 text-muted-foreground" />
                    <span className="min-w-0 flex-1 truncate text-sm">
                      {prettyCwd(entry.cwd)}
                    </span>
                    <span className="shrink-0 text-[11px] text-muted-foreground">
                      {formatWhen(entry.modifiedMs)}
                    </span>
                  </button>
                ))}
              </div>
            </div>
          )}

          {servers.length > 0 && (
            <div className="space-y-1.5">
              <Label className="flex items-center gap-1.5 text-xs">
                <PlugIcon className="size-3.5" />
                {t("MCP sunucuları")}
              </Label>
              <div className="flex flex-wrap gap-1.5">
                {servers.map((server) => {
                  const active = chosen === null || chosen.has(server.name);
                  return (
                    <button
                      className={cn(
                        "flex items-center gap-1.5 rounded-lg border px-2.5 py-1.5 text-left text-xs transition-colors",
                        active
                          ? "border-primary bg-primary/10"
                          : "text-muted-foreground hover:border-foreground/25",
                      )}
                      key={`${server.scope ?? "user"}:${server.name}`}
                      onClick={() => toggleServer(server.name)}
                      type="button"
                    >
                      <span
                        className={cn(
                          "grid size-3.5 shrink-0 place-items-center rounded border",
                          active ? "border-primary bg-primary" : "border-muted-foreground/40",
                        )}
                      >
                        {active && <CheckIcon className="size-2.5 text-primary-foreground" />}
                      </span>
                      {server.name}
                    </button>
                  );
                })}
              </div>
              <p className="text-[11px] text-muted-foreground leading-snug">
                {chosen === null || chosen.size === servers.length
                  ? t(
                      "Hepsi açık — genel yapılandırma, eklentilerin getirdiği sunucular dahil.",
                    )
                  : t(
                      "Yalnızca seçilenler bu sohbette açık; eklenti sunucuları da kapanır.",
                    )}
              </p>
            </div>
          )}
        </div>

        <DialogFooter>
          <Button disabled={!path.trim()} onClick={start}>
            {t("Başlat")}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
