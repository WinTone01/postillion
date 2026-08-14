import { useEffect } from "react";
import { FolderIcon, SettingsIcon, SparklesIcon, UserIcon } from "lucide-react";

import {
  CommandDialog,
  CommandEmpty,
  CommandGroup,
  CommandInput,
  CommandItem,
  CommandList,
} from "@/components/ui/command";
import { formatWhen, prettyCwd, type Account, type Session } from "@/api";
import { t } from "@/lib/i18n";

interface Props {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  sessions: Session[];
  accounts: Account[];
  onResume: (session: Session) => void;
  onSwitchAccount: (slug: string) => void;
  onNewSession: () => void;
  onOpenSettings: () => void;
}

/**
 * ⌘K / Ctrl+K komut paleti.
 *
 * 108 oturum arasında fareyle gezinmek yerine yazarak atlamayı sağlıyor;
 * masaüstü uygulamalarında beklenen bir davranış.
 */
export default function CommandPalette({
  open,
  onOpenChange,
  sessions,
  accounts,
  onResume,
  onSwitchAccount,
  onNewSession,
  onOpenSettings,
}: Props) {
  useEffect(() => {
    function onKey(event: KeyboardEvent) {
      if (event.key.toLowerCase() === "k" && (event.metaKey || event.ctrlKey)) {
        event.preventDefault();
        onOpenChange(!open);
      }
    }
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [open, onOpenChange]);

  function run(action: () => void) {
    onOpenChange(false);
    action();
  }

  return (
    <CommandDialog
      description={t("Oturum, hesap ve eylem arayın")}
      onOpenChange={onOpenChange}
      open={open}
      title={t("Komut paleti")}
    >
      <CommandInput placeholder={t("Oturum ara ya da komut yazın…")} />
      <CommandList>
        <CommandEmpty>{t("Sonuç yok.")}</CommandEmpty>

        <CommandGroup heading={t("Eylemler")}>
          <CommandItem onSelect={() => run(onNewSession)}>
            <SparklesIcon className="size-4" />
            {t("Yeni oturum başlat")}
          </CommandItem>
          <CommandItem onSelect={() => run(onOpenSettings)}>
            <SettingsIcon className="size-4" />
            {t("Ayarları aç")}
          </CommandItem>
        </CommandGroup>

        <CommandGroup heading={t("Hesaplar")}>
          {accounts.map((account) => (
            <CommandItem
              key={account.slug}
              onSelect={() => run(() => onSwitchAccount(account.slug))}
              value={`hesap ${account.label} ${account.email ?? ""}`}
            >
              <UserIcon className="size-4" />
              <span>{account.label}</span>
              <span className="ml-auto text-muted-foreground text-xs">
                {account.isActive ? t("etkin") : (account.email ?? "")}
              </span>
            </CommandItem>
          ))}
        </CommandGroup>

        <CommandGroup heading={t("Oturumlar")}>
          {/* cmdk tüm listeyi filtreliyor; 108 kaydın hepsini vermek yerine
              ilk 60'ı yeterli — arama zaten daraltıyor. */}
          {sessions.slice(0, 60).map((session) => (
            <CommandItem
              key={session.path}
              onSelect={() => run(() => onResume(session))}
              value={`${session.title ?? ""} ${session.cwd ?? ""} ${session.sessionId}`}
            >
              <FolderIcon className="size-4 shrink-0" />
              <span className="truncate">{session.title ?? session.sessionId}</span>
              <span className="ml-auto shrink-0 text-muted-foreground text-xs">
                {prettyCwd(session.cwd)} · {formatWhen(session.modifiedMs)}
              </span>
            </CommandItem>
          ))}
        </CommandGroup>
      </CommandList>
    </CommandDialog>
  );
}
