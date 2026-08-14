import { useMemo, useState } from "react";
import { motion } from "motion/react";
import {
  ArrowRightIcon,
  FolderIcon,
  GitBranchIcon,
  MessagesSquareIcon,
  PlusIcon,
  RefreshCwIcon,
  SearchIcon,
} from "lucide-react";

import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { cn } from "@/lib/utils";
import { formatBytes, formatWhen, prettyCwd, type Account, type Session } from "@/api";
import { t } from "@/lib/i18n";

interface Props {
  sessions: Session[];
  account: Account | undefined;
  loading: boolean;
  onResume: (session: Session) => void;
  onNew: () => void;
  onRefresh: () => void;
}

export default function SessionList({
  sessions,
  account,
  loading,
  onResume,
  onNew,
  onRefresh,
}: Props) {
  const [query, setQuery] = useState("");

  const filtered = useMemo(() => {
    const q = query.trim().toLocaleLowerCase("tr");
    if (!q) return sessions;
    return sessions.filter((s) =>
      [s.title, s.cwd, s.sessionId, s.gitBranch]
        .filter(Boolean)
        .some((field) => field!.toLocaleLowerCase("tr").includes(q)),
    );
  }, [sessions, query]);

  /**
   * Projeye göre grupla.
   *
   * Düz liste 108 oturumda okunmaz hale geliyordu; oturumlar zaten zihinsel
   * olarak projeye ait. Gruplar en yeni oturumuna göre sıralanıyor.
   */
  const groups = useMemo(() => {
    const byProject = new Map<string, Session[]>();
    for (const session of filtered) {
      const key = prettyCwd(session.cwd);
      const list = byProject.get(key);
      if (list) list.push(session);
      else byProject.set(key, [session]);
    }
    return [...byProject.entries()].sort(
      (a, b) => (b[1][0]?.modifiedMs ?? 0) - (a[1][0]?.modifiedMs ?? 0),
    );
  }, [filtered]);

  const canRun = account !== undefined;

  return (
    <div className="flex h-full flex-col">
      <header className="flex items-center gap-2 border-b px-5 py-3">
        <div className="relative flex-1">
          <SearchIcon className="-translate-y-1/2 pointer-events-none absolute top-1/2 left-3 size-4 text-muted-foreground" />
          <Input
            aria-label={t("Oturum ara")}
            className="h-9 pl-9"
            onChange={(e) => setQuery(e.target.value)}
            placeholder={t("Oturum ara — başlık, proje, dal")}
            value={query}
          />
        </div>

        <Button
          aria-label={t("Yenile")}
          disabled={loading}
          onClick={onRefresh}
          size="icon"
          variant="ghost"
        >
          <RefreshCwIcon className={cn("size-4", loading && "animate-spin")} />
        </Button>
        <Button disabled={!canRun} onClick={onNew} size="sm">
          <PlusIcon className="size-3.5" />
          {t("Yeni oturum")}
        </Button>
      </header>

      {!canRun && account && (
        <div className="mx-5 mt-4 flex items-start gap-2.5 rounded-xl border border-warning/25 bg-warning/10 px-4 py-3">
          <div className="mt-0.5 size-2 shrink-0 rounded-full bg-warning" />
          <div>
            <p className="font-medium text-sm">{t("Etkin hesap yok")}</p>
            <p className="mt-0.5 text-muted-foreground text-xs">
              {t(
                "Sol panelden giriş yapın — OAuth akışını Claude yürütür, token bu uygulamadan geçmez.",
              )}
            </p>
          </div>
        </div>
      )}

      <div className="flex-1 overflow-y-auto px-5 py-4">
        {loading && sessions.length === 0 && (
          <div className="space-y-2">
            {[0, 1, 2, 3].map((i) => (
              <div className="h-[68px] animate-pulse rounded-xl bg-muted/60" key={i} />
            ))}
          </div>
        )}

        {!loading && filtered.length === 0 && (
          <div className="mt-20 flex flex-col items-center text-center">
            <div className="grid size-12 place-items-center rounded-2xl bg-muted">
              <MessagesSquareIcon className="size-5 text-muted-foreground" />
            </div>
            <p className="mt-3 font-medium text-sm">
              {query ? t("Eşleşen oturum yok") : t("Henüz oturum yok")}
            </p>
            <p className="mt-1 max-w-xs text-muted-foreground text-xs">
              {query
                ? t("Farklı bir arama deneyin.")
                : t(
                    "Oturumlar ~/.claude/projects altından okunur. Yeni bir tane başlatın.",
                  )}
            </p>
          </div>
        )}

        <div className="space-y-6">
          {groups.map(([project, items]) => (
            <section key={project}>
              <div className="mb-2 flex items-center gap-2">
                <FolderIcon className="size-3.5 text-muted-foreground" />
                <h2 className="truncate font-medium text-muted-foreground text-xs">
                  {project}
                </h2>
                <span className="rounded bg-muted px-1.5 text-[10px] text-muted-foreground">
                  {items.length}
                </span>
                <div className="h-px flex-1 bg-border" />
              </div>

              <div className="space-y-1.5">
                {items.map((session, index) => (
                  <motion.button
                    animate={{ opacity: 1, y: 0 }}
                    className={cn(
                      "group flex w-full items-center gap-3 rounded-xl border bg-card px-4 py-3 text-left",
                      "transition-all hover:border-primary/35 hover:shadow-sm",
                      !canRun && "pointer-events-none opacity-60",
                    )}
                    disabled={!canRun}
                    initial={{ opacity: 0, y: 4 }}
                    key={session.path}
                    onClick={() => onResume(session)}
                    transition={{ delay: Math.min(index * 0.015, 0.15) }}
                    type="button"
                  >
                    <div className="min-w-0 flex-1">
                      <p className="truncate font-medium text-sm">
                        {session.title ?? session.sessionId}
                      </p>

                      <div className="mt-1 flex flex-wrap items-center gap-x-2.5 gap-y-1 text-[11px] text-muted-foreground">
                        <span>{formatWhen(session.modifiedMs)}</span>
                        <span className="opacity-40">·</span>
                        <span>{formatBytes(session.sizeBytes)}</span>
                        {session.gitBranch && session.gitBranch !== "HEAD" && (
                          <>
                            <span className="opacity-40">·</span>
                            <span className="inline-flex items-center gap-1">
                              <GitBranchIcon className="size-3" />
                              {session.gitBranch}
                            </span>
                          </>
                        )}
                        {session.model && (
                          <span className="rounded bg-muted px-1.5 py-px text-[10px]">
                            {session.model.replace(/^claude-/, "")}
                          </span>
                        )}
                      </div>
                    </div>

                    <ArrowRightIcon className="size-4 shrink-0 text-muted-foreground opacity-0 transition-all group-hover:translate-x-0.5 group-hover:opacity-100" />
                  </motion.button>
                ))}
              </div>
            </section>
          ))}
        </div>
      </div>
    </div>
  );
}
