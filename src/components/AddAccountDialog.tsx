import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { CheckIcon, ExternalLinkIcon, Loader2Icon } from "lucide-react";

import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { api, errText, type Account } from "@/api";
import { log } from "@/lib/log";

interface Props {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  onAdded: (account: Account) => void;
}

interface UrlEvent {
  url: string;
}

interface DoneEvent {
  ok: boolean;
  account: Account | null;
  error: string | null;
}

type Phase = "idle" | "starting" | "waiting" | "submitting" | "done";

/**
 * Uygulama içi giriş.
 *
 * `claude auth login` tarayıcıyı açıp URL basıyor, sonra stdin'den kod
 * bekliyor. Backend URL'yi `auth://url` ile yolluyor; kullanıcının yapıştırdığı
 * kodu geri yazıyoruz. Ayrı bir terminale gerek kalmıyor.
 *
 * Giriş geçici bir yapılandırma dizininde yapılıyor, yani yarıda bırakmak etkin
 * hesabı bozmuyor.
 */
export default function AddAccountDialog({ open, onOpenChange, onAdded }: Props) {
  const [phase, setPhase] = useState<Phase>("idle");
  const [url, setUrl] = useState<string | null>(null);
  const [code, setCode] = useState("");
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (!open) return;

    let disposed = false;
    const unlisteners: Array<() => void> = [];

    async function boot() {
      setPhase("starting");
      setUrl(null);
      setCode("");
      setError(null);

      const onUrl = await listen<UrlEvent>("auth://url", (e) => {
        setUrl(e.payload.url);
        setPhase("waiting");
      });
      const onDone = await listen<DoneEvent>("auth://done", (e) => {
        if (e.payload.ok && e.payload.account) {
          setPhase("done");
          onAdded(e.payload.account);
          onOpenChange(false);
        } else {
          setPhase("waiting");
          setError(e.payload.error ?? "giriş tamamlanamadı");
        }
      });

      if (disposed) {
        onUrl();
        onDone();
        return;
      }
      unlisteners.push(onUrl, onDone);

      try {
        await api.loginStart();
      } catch (e) {
        log("error", "giriş başlatılamadı:", e);
        setError(errText(e));
        setPhase("idle");
      }
    }

    void boot();

    return () => {
      disposed = true;
      unlisteners.forEach((un) => un());
      void api.loginCancel().catch(() => {});
    };
  }, [open, onAdded, onOpenChange]);

  async function submit() {
    if (!code.trim()) return;
    setPhase("submitting");
    setError(null);
    try {
      await api.loginSubmitCode(code.trim());
    } catch (e) {
      setError(errText(e));
      setPhase("waiting");
    }
  }

  return (
    <Dialog onOpenChange={onOpenChange} open={open}>
      <DialogContent className="sm:max-w-lg">
        <DialogHeader>
          <DialogTitle>Hesap ekle</DialogTitle>
          <DialogDescription>
            Tarayıcıda Anthropic hesabınıza giriş yapın, ardından verilen kodu
            buraya yapıştırın.
          </DialogDescription>
        </DialogHeader>

        <div className="space-y-4">
          {phase === "starting" && (
            <p className="flex items-center gap-2 text-muted-foreground text-sm">
              <Loader2Icon className="size-4 animate-spin" />
              Giriş bağlantısı hazırlanıyor…
            </p>
          )}

          {url && (
            <div className="space-y-2">
              <Label className="text-xs">1. Tarayıcıda açın</Label>
              <div className="flex gap-2">
                <Input className="font-mono text-xs" readOnly value={url} />
                <Button
                  onClick={() => window.open(url, "_blank")}
                  size="icon"
                  variant="secondary"
                >
                  <ExternalLinkIcon className="size-4" />
                </Button>
              </div>
              <p className="text-[11px] text-muted-foreground">
                Tarayıcı kendiliğinden açılmış olabilir. Açılmadıysa bu adresi
                kullanın.
              </p>
            </div>
          )}

          {(phase === "waiting" || phase === "submitting") && (
            <div className="space-y-2">
              <Label className="text-xs">2. Kodu yapıştırın</Label>
              <div className="flex gap-2">
                <Input
                  autoFocus
                  className="font-mono text-xs"
                  onChange={(e) => setCode(e.target.value)}
                  onKeyDown={(e) => {
                    if (e.key === "Enter") void submit();
                  }}
                  placeholder="giriş sonrası verilen kod"
                  value={code}
                />
                <Button
                  disabled={!code.trim() || phase === "submitting"}
                  onClick={() => void submit()}
                >
                  {phase === "submitting" ? (
                    <Loader2Icon className="size-4 animate-spin" />
                  ) : (
                    <CheckIcon className="size-4" />
                  )}
                  Tamamla
                </Button>
              </div>
            </div>
          )}

          {error && (
            <p className="rounded-lg border border-destructive/30 bg-destructive/10 px-3 py-2 text-destructive text-xs">
              {error}
            </p>
          )}
        </div>
      </DialogContent>
    </Dialog>
  );
}
