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

interface Props {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  sessions: Session[];
  accounts: Account[];
  onResume: (session: Session) => void;
  onSelectAccount: (name: string) => void;
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
  onSelectAccount,
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
      description="Oturum, hesap ve eylem arayın"
      onOpenChange={onOpenChange}
      open={open}
      title="Komut paleti"
    >
      <CommandInput placeholder="Oturum ara ya da komut yazın…" />
      <CommandList>
        <CommandEmpty>Sonuç yok.</CommandEmpty>

        <CommandGroup heading="Eylemler">
          <CommandItem onSelect={() => run(onNewSession)}>
            <SparklesIcon className="size-4" />
            Yeni oturum başlat
          </CommandItem>
          <CommandItem onSelect={() => run(onOpenSettings)}>
            <SettingsIcon className="size-4" />
            Ayarları aç
          </CommandItem>
        </CommandGroup>

        <CommandGroup heading="Hesaplar">
          {accounts.map((account) => (
            <CommandItem
              key={account.name}
              onSelect={() => run(() => onSelectAccount(account.name))}
              value={`hesap ${account.name} ${account.email ?? ""}`}
            >
              <UserIcon className="size-4" />
              <span>{account.name}</span>
              <span className="ml-auto text-muted-foreground text-xs">
                {account.email ?? "giriş yok"}
              </span>
            </CommandItem>
          ))}
        </CommandGroup>

        <CommandGroup heading="Oturumlar">
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
