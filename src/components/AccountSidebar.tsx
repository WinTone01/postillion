import { AnimatePresence, motion } from "motion/react";
import {
  AlertTriangleIcon,
  MessagesSquareIcon,
  PlusIcon,
  SettingsIcon,
  Trash2Icon,
} from "lucide-react";

import Mascot, { type MascotState } from "@/components/Mascot";
import { Button } from "@/components/ui/button";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import { cn } from "@/lib/utils";
import type { Account } from "@/api";

interface Props {
  accounts: Account[];
  /** Hesaba tıklamak sistem genelinde ona geçiyor. */
  onSwitch: (slug: string) => void;
  onAddAccount: () => void;
  onDelete: (slug: string) => void;
  onOpenSettings: () => void;
  onOpenSessions: () => void;
  /** Kenar çubuğundaki gezinmede hangisi seçili. */
  navActive: "sessions" | "settings" | null;
  /** Geçiş sürerken tıklamalar kilitleniyor; çift geçiş yarış yaratır. */
  busy: boolean;
  /** Maskotun yansıttığı genel uygulama durumu. */
  mascotState: MascotState;
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
  const source = account.label || account.email || account.slug;
  const parts = source.replace(/@.*$/, "").split(/[\s._-]+/).filter(Boolean);
  const letters = parts.slice(0, 2).map((p) => p[0]);
  return (letters.join("") || source.slice(0, 2)).toLocaleUpperCase("tr");
}

export default function AccountSidebar({
  accounts,
  onSwitch,
  onAddAccount,
  onDelete,
  onOpenSettings,
  onOpenSessions,
  navActive,
  mascotState,
  busy,
}: Props) {
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
        {accounts.length === 0 && (
          <p className="px-2.5 py-3 text-[11px] text-muted-foreground leading-snug">
            Henüz hesap yok. Aşağıdan ekleyin.
          </p>
        )}

        {accounts.map((account) => {
          const active = account.isActive;
          return (
            <div
              className={cn(
                "group relative rounded-xl px-2.5 py-2.5 transition-colors",
                active ? "bg-sidebar-accent" : "hover:bg-sidebar-accent/55",
                busy ? "pointer-events-none opacity-60" : "cursor-pointer",
              )}
              key={account.slug}
              onClick={() => !active && onSwitch(account.slug)}
              onKeyDown={(e) => {
                if ((e.key === "Enter" || e.key === " ") && !active) onSwitch(account.slug);
              }}
              role="button"
              tabIndex={0}
            >
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
                  {active && (
                    <span
                      aria-label="etkin hesap"
                      className="absolute -right-0.5 -bottom-0.5 size-2.5 rounded-full bg-success ring-2 ring-sidebar"
                    />
                  )}
                </div>

                <div className="min-w-0 flex-1">
                  <div className="flex items-center gap-1.5">
                    <span className="truncate font-medium text-sm">{account.label}</span>
                    {active && (
                      <span className="shrink-0 rounded bg-background/70 px-1 py-px text-[9px] text-muted-foreground uppercase tracking-wide">
                        etkin
                      </span>
                    )}
                  </div>
                  <p className="truncate text-[11px] text-muted-foreground">
                    {account.email ?? account.slug}
                  </p>
                </div>
              </div>

              {!account.hasCredentials && (
                <p className="mt-1.5 flex items-center gap-1 pl-[42px] text-[11px] text-warning">
                  <AlertTriangleIcon className="size-3 shrink-0" />
                  oturum yok — yeniden giriş gerekiyor
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
                    <p className="pt-1.5 pl-[42px] text-[11px] text-muted-foreground leading-snug">
                      Terminaldeki <code>claude</code> de bu hesabı kullanıyor.
                    </p>
                  </motion.div>
                )}
              </AnimatePresence>

              {!active && (
                <div className="absolute top-2 right-2 opacity-0 transition-opacity group-hover:opacity-100">
                  <IconAction
                    destructive
                    icon={<Trash2Icon className="size-3.5" />}
                    label="Hesabı kaldır"
                    onClick={() => onDelete(account.slug)}
                  />
                </div>
              )}
            </div>
          );
        })}
      </div>

      <div className="border-t p-2.5">
        <Button
          className="w-full"
          disabled={busy}
          onClick={onAddAccount}
          size="sm"
          variant="secondary"
        >
          <PlusIcon className="size-3.5" />
          Hesap ekle
        </Button>
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
