import { useState } from "react";
import { AnimatePresence, motion } from "motion/react";
import {
  AlertTriangleIcon,
  CheckIcon,
  LogInIcon,
  MessagesSquareIcon,
  PlusIcon,
  SettingsIcon,
  Trash2Icon,
  WrenchIcon,
  XIcon,
} from "lucide-react";

import Mascot, { type MascotState } from "@/components/Mascot";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import { cn } from "@/lib/utils";
import type { Account } from "@/api";

interface Props {
  accounts: Account[];
  selected: string;
  onSelect: (name: string) => void;
  onCreate: (name: string) => void;
  onLogin: (name: string) => void;
  onRepair: (name: string) => void;
  onDelete: (name: string) => void;
  onOpenSettings: () => void;
  onOpenSessions: () => void;
  /** Kenar çubuğundaki gezinmede hangisi seçili. */
  navActive: "sessions" | "settings" | null;
  /** Maskotun yansıttığı genel uygulama durumu. */
  mascotState: MascotState;
  busy: boolean;
}

/** Maskot süs değil, durum göstergesi — ekran okuyucu da görebilmeli. */
const MASCOT_LABELS: Record<MascotState, string> = {
  idle: "Boşta",
  thinking: "Claude düşünüyor",
  working: "Araç çalışıyor",
  waiting: "İzin bekleniyor",
  error: "Hata var",
};

/** İsimden iki harflik baş harf; avatar için. */
function initials(account: Account): string {
  const source = account.displayName || account.email || account.name;
  const parts = source.replace(/@.*$/, "").split(/[\s._-]+/).filter(Boolean);
  const letters = parts.slice(0, 2).map((p) => p[0]);
  return (letters.join("") || source.slice(0, 2)).toLocaleUpperCase("tr");
}

export default function AccountSidebar({
  accounts,
  selected,
  onSelect,
  onCreate,
  onLogin,
  onRepair,
  onDelete,
  onOpenSettings,
  onOpenSessions,
  navActive,
  mascotState,
  busy,
}: Props) {
  const [adding, setAdding] = useState(false);
  const [name, setName] = useState("");

  function submit() {
    const trimmed = name.trim();
    if (!trimmed) return;
    onCreate(trimmed);
    setName("");
    setAdding(false);
  }

  return (
    <aside className="flex h-full w-[272px] shrink-0 flex-col border-r bg-sidebar">
      <div className="flex items-center gap-3 px-4 pt-4 pb-4">
        {/* Maskot taşmadan hareket edebilsin diye kutu figürden belirgin
            biçimde büyük; overflow-hidden yok. */}
        <div className="grid size-12 shrink-0 place-items-center rounded-2xl bg-primary">
          <Mascot className="size-9" label={MASCOT_LABELS[mascotState]} state={mascotState} />
        </div>
        <div className="min-w-0">
          <h1 className="truncate font-semibold text-lg leading-tight tracking-tight">
            Postillion
          </h1>
          <p className="truncate text-muted-foreground text-xs leading-tight">
            Hesaplar arası oturum devamı
          </p>
        </div>
      </div>

      {/* Gezinme sekme çubuğundan buraya taşındı: üst çubuk artık yalnızca
          açık sohbetlere ait, karışmıyor. */}
      <nav className="space-y-0.5 px-2 pb-2">
        <NavItem
          active={navActive === "sessions"}
          icon={<MessagesSquareIcon className="size-4" />}
          label="Oturumlar"
          onClick={onOpenSessions}
        />
        <NavItem
          active={navActive === "settings"}
          icon={<SettingsIcon className="size-4" />}
          label="Ayarlar"
          onClick={onOpenSettings}
        />
      </nav>

      <p className="px-4 pt-2 pb-2 font-medium text-[11px] text-muted-foreground uppercase tracking-wider">
        Hesaplar
      </p>

      <div className="flex-1 space-y-1 overflow-y-auto px-2 pb-2">
        {accounts.map((account) => {
          const active = account.name === selected;
          return (
            <div
              className={cn(
                "group relative cursor-pointer rounded-xl px-2.5 py-2.5 transition-colors",
                active ? "bg-sidebar-accent" : "hover:bg-sidebar-accent/55",
              )}
              key={account.name}
              onClick={() => onSelect(account.name)}
              onKeyDown={(e) => {
                if (e.key === "Enter" || e.key === " ") onSelect(account.name);
              }}
              role="button"
              tabIndex={0}
            >
              {/* Seçili hesabı sol kenardaki şeritle işaretliyoruz; renkli arka
                  plan tek başına yeterince okunur değildi. */}
              {active && (
                <motion.span
                  className="absolute top-2.5 bottom-2.5 -left-0.5 w-[3px] rounded-full bg-primary"
                  layoutId="account-marker"
                  transition={{ type: "spring", stiffness: 500, damping: 40 }}
                />
              )}

              <div className="flex items-center gap-2.5">
                <div className="relative shrink-0">
                  <div
                    className={cn(
                      "grid size-8 place-items-center rounded-full font-medium text-xs",
                      active
                        ? "bg-primary text-primary-foreground"
                        : "bg-muted text-muted-foreground",
                    )}
                  >
                    {initials(account)}
                  </div>
                  <span
                    aria-label={account.loggedIn ? "giriş yapılmış" : "giriş yok"}
                    className={cn(
                      "absolute -right-0.5 -bottom-0.5 size-2.5 rounded-full ring-2 ring-sidebar",
                      account.loggedIn ? "bg-success" : "bg-muted-foreground/50",
                    )}
                  />
                </div>

                <div className="min-w-0 flex-1">
                  <div className="flex items-center gap-1.5">
                    <span className="truncate font-medium text-sm">{account.name}</span>
                    {account.isDefault && (
                      <span className="shrink-0 rounded bg-background/70 px-1 py-px text-[9px] text-muted-foreground uppercase tracking-wide">
                        kaynak
                      </span>
                    )}
                  </div>
                  <p className="truncate text-[11px] text-muted-foreground">
                    {account.email ?? "giriş yapılmamış"}
                  </p>
                </div>
              </div>

              {account.brokenLinks.length > 0 && (
                <p className="mt-1.5 flex items-center gap-1 pl-[42px] text-[11px] text-warning">
                  <AlertTriangleIcon className="size-3 shrink-0" />
                  {account.brokenLinks.length} bağlantı kırık
                </p>
              )}

              <AnimatePresence initial={false}>
                {active && (
                  <motion.div
                    animate={{ height: "auto", opacity: 1 }}
                    className="overflow-hidden"
                    exit={{ height: 0, opacity: 0 }}
                    initial={{ height: 0, opacity: 0 }}
                    transition={{ duration: 0.16 }}
                  >
                    <div className="flex gap-1 pt-2 pl-[42px]">
                      {!account.loggedIn && (
                        <IconAction
                          disabled={busy}
                          icon={<LogInIcon className="size-3.5" />}
                          label="Giriş yap"
                          onClick={() => onLogin(account.name)}
                        />
                      )}
                      {account.brokenLinks.length > 0 && (
                        <IconAction
                          disabled={busy}
                          icon={<WrenchIcon className="size-3.5" />}
                          label="Bağlantıları onar"
                          onClick={() => onRepair(account.name)}
                        />
                      )}
                      {!account.isDefault && (
                        <IconAction
                          destructive
                          disabled={busy}
                          icon={<Trash2Icon className="size-3.5" />}
                          label="Hesabı sil"
                          onClick={() => onDelete(account.name)}
                        />
                      )}
                    </div>
                  </motion.div>
                )}
              </AnimatePresence>
            </div>
          );
        })}
      </div>

      <div className="border-t p-2.5">
        <AnimatePresence initial={false} mode="wait">
          {adding ? (
            <motion.div
              animate={{ opacity: 1, y: 0 }}
              className="space-y-2"
              initial={{ opacity: 0, y: 4 }}
              key="form"
            >
              <Input
                autoFocus
                aria-label="Yeni hesap ismi"
                className="h-8 text-sm"
                onChange={(e) => setName(e.target.value)}
                onKeyDown={(e) => {
                  if (e.key === "Enter") submit();
                  if (e.key === "Escape") setAdding(false);
                }}
                placeholder="hesap-ismi"
                value={name}
              />
              <div className="flex gap-1.5">
                <Button className="flex-1" disabled={busy} onClick={submit} size="sm">
                  <CheckIcon className="size-3.5" />
                  Oluştur
                </Button>
                <Button onClick={() => setAdding(false)} size="sm" variant="ghost">
                  <XIcon className="size-3.5" />
                </Button>
              </div>
              <p className="text-[10.5px] text-muted-foreground leading-snug">
                Proje onayları ve MCP ayarları kopyalanır; kimlik bilgileri
                kopyalanmaz.
              </p>
            </motion.div>
          ) : (
            <motion.div animate={{ opacity: 1 }} initial={{ opacity: 0 }} key="buttons">
              <Button
                className="w-full"
                disabled={busy}
                onClick={() => setAdding(true)}
                size="sm"
                variant="secondary"
              >
                <PlusIcon className="size-3.5" />
                Hesap ekle
              </Button>
            </motion.div>
          )}
        </AnimatePresence>
      </div>
    </aside>
  );
}

/** Kenar çubuğu gezinme öğesi. */
function NavItem({
  icon,
  label,
  active,
  onClick,
}: {
  icon: React.ReactNode;
  label: string;
  active: boolean;
  onClick: () => void;
}) {
  return (
    <button
      className={cn(
        "flex w-full items-center gap-2.5 rounded-lg px-2.5 py-2 text-left text-sm transition-colors",
        active
          ? "bg-sidebar-accent font-medium"
          : "text-muted-foreground hover:bg-sidebar-accent/55 hover:text-foreground",
      )}
      onClick={onClick}
      type="button"
    >
      {icon}
      {label}
    </button>
  );
}

/** Etiketi tooltip'te taşıyan küçük eylem butonu. */
function IconAction({
  icon,
  label,
  onClick,
  disabled,
  destructive,
}: {
  icon: React.ReactNode;
  label: string;
  onClick: () => void;
  disabled?: boolean;
  destructive?: boolean;
}) {
  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <button
          aria-label={label}
          className={cn(
            "grid size-7 place-items-center rounded-lg transition-colors",
            "bg-background/60 text-muted-foreground hover:bg-background",
            destructive && "hover:text-destructive",
            disabled && "pointer-events-none opacity-50",
          )}
          onClick={(e) => {
            e.stopPropagation();
            onClick();
          }}
          type="button"
        >
          {icon}
        </button>
      </TooltipTrigger>
      <TooltipContent>{label}</TooltipContent>
    </Tooltip>
  );
}
