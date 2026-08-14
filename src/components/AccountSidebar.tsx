import { motion } from "motion/react";
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
import { formatWhen, type Account, type Usage } from "@/api";
import { formatUntil, percent, t } from "@/lib/i18n";
import { parseResetAt, resetWindow } from "@/lib/usage";

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
  /**
   * Bir sohbet sürüyor: hesap değiştirmek ve eklemek kapalı.
   *
   * Hesap değişimi paylaşılan kimlik dosyasını değiştiriyor ve çalışan bir
   * `claude` süreci onu altından çekilince bozulur.
   */
  locked: boolean;
  /** Maskotun yansıttığı genel uygulama durumu. */
  mascotState: MascotState;
  /** Hesap kısa adı → son kullanım ölçümü. */
  usage: Record<string, Usage>;
}

/** Maskot süs değil, durum göstergesi — ekran okuyucu da görebilmeli. */
function mascotLabel(state: MascotState): string {
  switch (state) {
    case "thinking":
      return t("Claude düşünüyor");
    case "working":
      return t("Araç çalışıyor");
    case "waiting":
      return t("İzin bekleniyor");
    case "error":
      return t("Hata var");
    default:
      return t("Boşta");
  }
}

/** `/usage` etiketleri İngilizce geliyor; bilinenler çevriliyor. */
function windowLabel(label: string): string {
  switch (label) {
    case "session":
      return t("Oturum");
    case "week (all models)":
      return t("Hafta");
    case "week (Opus)":
      return t("Hafta · Opus");
    default:
      return label;
  }
}

/**
 * Kullanım göstergesi.
 *
 * En dar pencere (oturum) çubuk olarak, kalanlar yanında yüzde olarak
 * gösteriliyor: karar anında bakılan şey "şimdi ne kadar payım var".
 *
 * Satırın sonunda payın ne zaman geri geleceği yazıyor. Önceden orada ölçümün
 * yaşı vardı ama o kullanıcının sorusuna cevap vermiyor — "ne zaman yine
 * yazabilirim" veriyor. Ölçüm zamanı ipucu kartına taşındı.
 *
 * Etkin olmayan hesaplarda değer ölçülemiyor: `claude` kimliği paylaşılan
 * dosyadan okuyor ve sorgulamak için o hesaba geçmek gerekirdi.
 */
function UsageMeter({ usage, stale }: { usage: Usage | undefined; stale: boolean }) {
  if (!usage || usage.windows.length === 0) return null;

  const [primary, ...rest] = usage.windows;
  // Sıcaklık eşikleri: %85 üstü "bugün bitebilir" demek.
  const tone =
    primary.percent >= 85 ? "bg-destructive" : primary.percent >= 60 ? "bg-warning" : "bg-success";

  const summary = usage.windows
    .map((w) => `${windowLabel(w.label)} ${percent(w.percent)}`)
    .join(" · ");

  const resets = usage.windows
    .filter((w) => w.resets)
    .map((w) => `${windowLabel(w.label)}: ${w.resets}`);

  /**
   * "3 sa sonra yenilenir". Haftalık pay bittiyse oturumun değil haftanın
   * sıfırlanması gösteriliyor — asıl engel o.
   */
  const renewal = (() => {
    const window = resetWindow(usage);
    if (!window?.resets) return null;

    const at = parseResetAt(window.resets);
    // Biçim beklenmedikse ham metni göster; hiç göstermemekten iyi.
    const when = at === null ? window.resets : formatUntil(at);
    return window.label.startsWith("week")
      ? t("hafta {when} yenilenir", { when })
      : t("{when} yenilenir", { when });
  })();

  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <div className={cn("mt-2 pl-[42px]", stale && "opacity-55")}>
          <div className="h-1 w-full overflow-hidden rounded-full bg-muted">
            <div
              className={cn("h-full rounded-full transition-[width]", tone)}
              style={{ width: `${Math.min(100, Math.max(2, primary.percent))}%` }}
            />
          </div>
          <p className="mt-1 truncate text-[10.5px] text-muted-foreground">
            {windowLabel(primary.label)} {percent(primary.percent)}
            {rest.length > 0 &&
              ` · ${rest.map((w) => `${windowLabel(w.label)} ${percent(w.percent)}`).join(" · ")}`}
            {renewal && ` · ${renewal}`}
          </p>
        </div>
      </TooltipTrigger>
      <TooltipContent className="max-w-[260px]" side="right">
        <p className="font-medium">{summary}</p>
        {resets.map((line) => (
          <p className="text-[11px] opacity-80" key={line}>
            {t("sıfırlanma")} — {line}
          </p>
        ))}
        <p className="mt-1 text-[11px] opacity-70">
          {stale
            ? t(
                "En son etkin olduğunda ölçüldü ({when}). Bu hesaba geçince güncellenir.",
                { when: formatWhen(usage.measuredAtMs) },
              )
            : t("{when} ölçüldü", { when: formatWhen(usage.measuredAtMs) })}
        </p>
      </TooltipContent>
    </Tooltip>
  );
}

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
  locked,
  usage,
}: Props) {
  const frozen = busy || locked;
  return (
    <aside className="flex h-full w-[272px] shrink-0 flex-col border-r bg-sidebar">
      <div className="flex items-center gap-3 px-4 pt-4 pb-4">
        {/* Maskot taşmadan hareket edebilsin diye kutu figürden belirgin
            biçimde büyük; overflow-hidden yok. */}
        <div className="grid size-12 shrink-0 place-items-center rounded-2xl bg-primary">
          <Mascot className="size-9" label={mascotLabel(mascotState)} state={mascotState} />
        </div>
        <div className="min-w-0">
          <h1 className="truncate font-semibold text-lg leading-tight tracking-tight">
            Postillion
          </h1>
          <p className="truncate text-muted-foreground text-xs leading-tight">
            {t("Aynı sohbet, istediğin hesapla")}
          </p>
        </div>
      </div>

      {/* Gezinme sekme çubuğundan buraya taşındı: üst çubuk artık yalnızca
          açık sohbetlere ait, karışmıyor. */}
      <nav className="space-y-0.5 px-2 pb-2">
        <NavItem
          active={navActive === "sessions"}
          icon={<MessagesSquareIcon className="size-4" />}
          label={t("Oturumlar")}
          onClick={onOpenSessions}
        />
        <NavItem
          active={navActive === "settings"}
          icon={<SettingsIcon className="size-4" />}
          label={t("Ayarlar")}
          onClick={onOpenSettings}
        />
      </nav>

      <p className="px-4 pt-2 pb-2 font-medium text-[11px] text-muted-foreground uppercase tracking-wider">
        {t("Hesaplar")}
      </p>

      {locked && (
        <p className="mx-2 mb-1.5 rounded-lg bg-muted/60 px-2.5 py-2 text-[11px] text-muted-foreground leading-snug">
          {t(
            "Bir sohbet sürüyor. Hesap değiştirmek paylaşılan kimlik dosyasını değiştirdiği için çalışan oturumu bozar; bitmesini bekleyin.",
          )}
        </p>
      )}

      <div className="flex-1 space-y-1 overflow-y-auto px-2 pb-2">
        {accounts.length === 0 && (
          <p className="px-2.5 py-3 text-[11px] text-muted-foreground leading-snug">
            {t("Henüz hesap yok. Aşağıdan ekleyin.")}
          </p>
        )}

        {accounts.map((account) => {
          const active = account.isActive;
          return (
            <div
              className={cn(
                "group relative rounded-xl px-2.5 py-2.5 transition-colors",
                active ? "bg-sidebar-accent" : "hover:bg-sidebar-accent/55",
                frozen ? "pointer-events-none opacity-60" : "cursor-pointer",
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
                      aria-label={t("etkin hesap")}
                      className="absolute -right-0.5 -bottom-0.5 size-2.5 rounded-full bg-success ring-2 ring-sidebar"
                    />
                  )}
                </div>

                <div className="min-w-0 flex-1">
                  <div className="flex items-center gap-1.5">
                    <span className="truncate font-medium text-sm">{account.label}</span>
                    {active && (
                      <span className="shrink-0 rounded bg-background/70 px-1 py-px text-[9px] text-muted-foreground uppercase tracking-wide">
                        {t("etkin")}
                      </span>
                    )}
                  </div>
                  <p className="truncate text-[11px] text-muted-foreground">
                    {account.email ?? account.slug}
                  </p>
                </div>
              </div>

              <UsageMeter stale={!active} usage={usage[account.slug]} />

              {!account.hasCredentials && (
                <p className="mt-1.5 flex items-center gap-1 pl-[42px] text-[11px] text-warning">
                  <AlertTriangleIcon className="size-3 shrink-0" />
                  {t("oturum yok — yeniden giriş gerekiyor")}
                </p>
              )}

              {!active && (
                <div className="absolute top-2 right-2 opacity-0 transition-opacity group-hover:opacity-100">
                  <IconAction
                    destructive
                    icon={<Trash2Icon className="size-3.5" />}
                    label={t("Hesabı kaldır")}
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
          disabled={frozen}
          onClick={onAddAccount}
          size="sm"
          variant="secondary"
        >
          <PlusIcon className="size-3.5" />
          {t("Hesap ekle")}
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
