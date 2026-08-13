import { useCallback, useEffect, useMemo, useState } from "react";
import { toast } from "sonner";
import { CommandIcon, XIcon } from "lucide-react";

import AccountSidebar from "@/components/AccountSidebar";
import ChatView from "@/components/ChatView";
import SessionList from "@/components/SessionList";
import CommandPalette from "@/components/CommandPalette";
import type { MascotState } from "@/components/Mascot";
import { Toaster } from "@/components/ui/sonner";
import { TooltipProvider } from "@/components/ui/tooltip";
import SettingsView from "@/components/SettingsView";
import { api, errText, type Account, type Preferences, type Session } from "@/api";
import { releaseAgentSession, type AgentSessionOptions } from "@/hooks/useAgentSession";
import { cn } from "@/lib/utils";

interface Tab {
  options: AgentSessionOptions;
  title: string;
  gitBranch: string | null;
}

type View = { kind: "sessions" } | { kind: "settings" } | { kind: "chat"; id: string };

export default function App() {
  const [accounts, setAccounts] = useState<Account[]>([]);
  const [selected, setSelected] = useState("default");
  const [sessions, setSessions] = useState<Session[]>([]);
  const [tabs, setTabs] = useState<Tab[]>([]);
  const [view, setView] = useState<View>({ kind: "sessions" });
  const [loading, setLoading] = useState(true);
  const [busy, setBusy] = useState(false);
  // Hatalar artık toast; satır içi çubuk düzeni kaydırıyordu.
  const notifyError = useCallback((message: string) => {
    toast.error(message);
  }, []);
  const [paletteOpen, setPaletteOpen] = useState(false);
  // Yeni sekmeler bu tercihlerle başlatılıyor (efor sonradan değiştirilemiyor).
  const [prefs, setPrefs] = useState<Preferences>({});
  // Sekme başına maskot durumu; kenar çubuğu en dikkat çekeni gösteriyor.
  const [tabStates, setTabStates] = useState<Record<string, MascotState>>({});

  const reportTabState = useCallback((id: string, next: MascotState) => {
    setTabStates((prev) => (prev[id] === next ? prev : { ...prev, [id]: next }));
  }, []);

  // Öncelik sırası: en çok ilgi isteyen durum kazanır.
  const mascotState: MascotState = useMemo(() => {
    const values = Object.values(tabStates);
    for (const candidate of ["error", "waiting", "working", "thinking"] as const) {
      if (values.includes(candidate)) return candidate;
    }
    return "idle";
  }, [tabStates]);

  const account = useMemo(
    () => accounts.find((a) => a.name === selected),
    [accounts, selected],
  );

  const loadAccounts = useCallback(async () => {
    try {
      setAccounts(await api.listAccounts());
    } catch (e) {
      notifyError(errText(e));
    }
  }, []);

  const loadSessions = useCallback(async (fresh = false) => {
    setLoading(true);
    try {
      setSessions(fresh ? await api.refreshSessions() : await api.listSessions());
    } catch (e) {
      notifyError(errText(e));
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    void loadAccounts();
    void loadSessions();
  }, [loadAccounts, loadSessions]);

  // Hesap değişince tercihler de değişir: settings.json hesap kapsamlı.
  useEffect(() => {
    api
      .readPreferences(selected)
      .then(setPrefs)
      .catch(() => setPrefs({}));
  }, [selected]);

  function openTab(tab: Tab) {
    setTabs((prev) => [...prev, tab]);
    setView({ kind: "chat", id: tab.options.id });
  }

  function closeTab(id: string) {
    void api.agentStop(id).catch(() => {});
    releaseAgentSession(id);
    setTabStates((prev) => {
      const next = { ...prev };
      delete next[id];
      return next;
    });
    setTabs((prev) => prev.filter((t) => t.options.id !== id));
    setView({ kind: "sessions" });
  }

  /** Projenin asıl amacı: aynı transcript, seçili hesapla devam. */
  function resume(session: Session) {
    if (!account?.loggedIn) return;

    openTab({
      options: {
        id: `resume-${session.sessionId}-${Date.now()}`,
        account: account.name,
        // Claude transcript'leri cwd'ye göre dizinliyor; aynı dizinden
        // başlatmazsak --resume oturumu bulamaz.
        cwd: session.cwd,
        resume: session.sessionId,
        // Geçmiş buradan yükleniyor; --resume onu tekrar yayınlamıyor.
        transcriptPath: session.path,
        model: prefs.model ?? null,
        effort: prefs.effortLevel ?? null,
      },
      title: session.title ?? session.sessionId,
      gitBranch: session.gitBranch,
    });
  }

  function newSession() {
    if (!account?.loggedIn) return;
    openTab({
      options: {
        id: `new-${Date.now()}`,
        account: account.name,
        cwd: null,
        resume: null,
        transcriptPath: null,
        model: prefs.model ?? null,
        effort: prefs.effortLevel ?? null,
      },
      title: `${account.name} · yeni oturum`,
      gitBranch: null,
    });
  }

  async function createAccount(name: string) {
    setBusy(true);
    try {
      const created = await api.createAccount(name);
      await loadAccounts();
      setSelected(created.name);
      // Yeni hesap kimliksiz doğar; giriş akışını hemen başlat.
      await api.accountLogin(created.name);
    } catch (e) {
      notifyError(errText(e));
    } finally {
      setBusy(false);
    }
  }

  async function login(name: string) {
    setBusy(true);
    try {
      await api.accountLogin(name);
      toast.success("Giriş akışı açıldı", {
        description: "Tamamlayınca hesap listesi otomatik yenilenecek.",
      });
      setTimeout(() => void loadAccounts(), 1500);
    } catch (e) {
      notifyError(errText(e));
    } finally {
      setBusy(false);
    }
  }

  async function repair(name: string) {
    setBusy(true);
    try {
      await api.repairAccount(name);
      await loadAccounts();
    } catch (e) {
      notifyError(errText(e));
    } finally {
      setBusy(false);
    }
  }

  async function remove(name: string) {
    setBusy(true);
    try {
      await api.deleteAccount(name);
      await loadAccounts();
      if (selected === name) setSelected("default");
    } catch (e) {
      notifyError(errText(e));
    } finally {
      setBusy(false);
    }
  }

  return (
    <TooltipProvider>
      <div className="flex h-full bg-background text-foreground">
        <AccountSidebar
          accounts={accounts}
          busy={busy}
          onCreate={createAccount}
          onDelete={remove}
          onLogin={login}
          mascotState={mascotState}
          navActive={
            view.kind === "sessions" ? "sessions" : view.kind === "settings" ? "settings" : null
          }
          onOpenSessions={() => setView({ kind: "sessions" })}
          onOpenSettings={() => setView({ kind: "settings" })}
          onRepair={repair}
          onSelect={setSelected}
          selected={selected}
        />

        <main className="flex min-w-0 flex-1 flex-col">
          {/* Üst çubuk artık yalnızca açık sohbetlere ait; gezinme kenar
              çubuğuna taşındı. */}
          <div className="flex h-11 items-center gap-1 overflow-x-auto border-b bg-sidebar px-3">
              {/* Sekmeler soldan büyür; palet kısayolu sağda sabit durur. */}
              {tabs.map((tab) => (
                <div
                  className={cn(
                    "flex shrink-0 items-center gap-1 rounded-md py-1.5 pr-1.5 pl-3 text-xs transition-colors",
                    view.kind === "chat" && view.id === tab.options.id
                      ? "bg-sidebar-accent"
                      : "text-muted-foreground hover:bg-sidebar-accent/60",
                  )}
                  key={tab.options.id}
                >
                  <button
                    className="max-w-[180px] truncate"
                    onClick={() => setView({ kind: "chat", id: tab.options.id })}
                    type="button"
                  >
                    {tab.title}
                  </button>
                  <button
                    aria-label="Sekmeyi kapat"
                    className="rounded p-0.5 hover:bg-background/60"
                    onClick={() => closeTab(tab.options.id)}
                    type="button"
                  >
                    <XIcon className="size-3" />
                  </button>
                </div>
              ))}

              <button
                className="ml-auto flex shrink-0 items-center gap-1 rounded-md px-2 py-1 text-[11px] text-muted-foreground transition-colors hover:bg-sidebar-accent/60"
                onClick={() => setPaletteOpen(true)}
                type="button"
              >
                <CommandIcon className="size-3" />
                K
              </button>
          </div>

          <div className="min-h-0 flex-1">
            <div
              className="h-full"
              style={{ display: view.kind === "sessions" ? "block" : "none" }}
            >
              <SessionList
                account={account}
                loading={loading}
                onNew={newSession}
                onRefresh={() => void loadSessions(true)}
                onResume={resume}
                sessions={sessions}
              />
            </div>

            <div
              className="h-full"
              style={{ display: view.kind === "settings" ? "block" : "none" }}
            >
              <SettingsView
                account={selected}
                onError={notifyError}
              />
            </div>

            {/* Sekmeler mount'ta kalır; gizlenince akış durumu kaybolmasın. */}
            {tabs.map((tab) => (
              <div
                className="h-full"
                key={tab.options.id}
                style={{
                  display:
                    view.kind === "chat" && view.id === tab.options.id ? "block" : "none",
                }}
              >
                <ChatView
                  gitBranch={tab.gitBranch}
                  onStateChange={reportTabState}
                  options={tab.options}
                  title={tab.title}
                />
              </div>
            ))}
          </div>
        </main>
      </div>

      <CommandPalette
        accounts={accounts}
        onNewSession={newSession}
        onOpenChange={setPaletteOpen}
        onOpenSettings={() => setView({ kind: "settings" })}
        onResume={resume}
        onSelectAccount={setSelected}
        open={paletteOpen}
        sessions={sessions}
      />

      {/* Uygulama koyu temaya sabit; next-themes sağlayıcısı yok, o yüzden
          temayı açıkça geçiyoruz (aksi halde "system"e düşüp açık renk olurdu). */}
      <Toaster position="bottom-right" richColors theme="dark" />
    </TooltipProvider>
  );
}
