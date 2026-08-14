import { useEffect, useMemo, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { FolderIcon, FolderOpenIcon } from "lucide-react";

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
import { formatWhen, prettyCwd, type Session } from "@/api";
import { log } from "@/lib/log";

interface Props {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  /** Oturum geçmişi; son kullanılan projeleri buradan çıkarıyoruz. */
  sessions: Session[];
  onStart: (cwd: string) => void;
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
  }, [isOpen, recent]);

  async function browse() {
    try {
      const picked = await open({
        directory: true,
        multiple: false,
        title: "Çalışma dizini seçin",
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
    onStart(trimmed);
    onOpenChange(false);
  }

  return (
    <Dialog onOpenChange={onOpenChange} open={isOpen}>
      <DialogContent className="sm:max-w-lg">
        <DialogHeader>
          <DialogTitle>Yeni oturum</DialogTitle>
          <DialogDescription>
            Claude bu dizinde çalışacak — dosyaları burada arar ve oturum buraya
            kaydedilir.
          </DialogDescription>
        </DialogHeader>

        <div className="space-y-4">
          <div className="space-y-1.5">
            <Label className="text-xs">Çalışma dizini</Label>
            <div className="flex gap-2">
              <Input
                className="font-mono text-xs"
                onChange={(e) => setPath(e.target.value)}
                onKeyDown={(e) => {
                  if (e.key === "Enter") start();
                }}
                placeholder="/home/kullanici/Projects/proje"
                value={path}
              />
              <Button onClick={() => void browse()} size="icon" variant="secondary">
                <FolderOpenIcon className="size-4" />
              </Button>
            </div>
          </div>

          {recent.length > 0 && (
            <div className="space-y-1.5">
              <Label className="text-xs">Son kullanılanlar</Label>
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
        </div>

        <DialogFooter>
          <Button disabled={!path.trim()} onClick={start}>
            Başlat
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
