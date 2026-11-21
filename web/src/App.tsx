import { useCallback, useEffect, useState } from "react";
import { FolderTree, Gauge, History as HistoryIcon, Settings as SettingsIcon, Sparkles, ExternalLink } from "lucide-react";
import { api, hasToken, type AppConfig } from "./api";
import Wizard from "./components/Wizard";
import Dashboard from "./components/Dashboard";
import Explorer from "./components/Explorer";
import Organize from "./components/Organize";
import History from "./components/History";
import Settings from "./components/Settings";

export type Page = "dashboard" | "explorer" | "organize" | "history" | "settings";

const NAV: { id: Page; label: string; icon: typeof Gauge }[] = [
  { id: "dashboard", label: "Übersicht", icon: Gauge },
  { id: "explorer", label: "Explorer", icon: FolderTree },
  { id: "organize", label: "Aufräumen", icon: Sparkles },
  { id: "history", label: "Verlauf", icon: HistoryIcon },
  { id: "settings", label: "Einstellungen", icon: SettingsIcon },
];

export default function App() {
  const [config, setConfig] = useState<AppConfig | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [page, setPage] = useState<Page>("dashboard");
  const [organizeTarget, setOrganizeTarget] = useState<string | null>(null);
  const [autoScan, setAutoScan] = useState(false);

  const reload = useCallback(() => {
    api.get<AppConfig>("/config").then(setConfig).catch((e) => setError(String(e.message ?? e)));
  }, []);

  useEffect(() => {
    if (hasToken) reload();
  }, [reload]);

  if (!hasToken || error) {
    return (
      <div className="center-screen">
        <div className="card narrow">
          <img src="/logo.png" width={64} alt="" />
          <h2>DriveSage</h2>
          <p className="muted">
            {error ?? "Diese Seite muss über das DriveSage-Programm geöffnet werden (aus Sicherheitsgründen mit einem Zugangs-Token)."}
          </p>
          <p className="muted small">Starte DriveSage neu oder nutze „Im Browser öffnen“ in der App.</p>
        </div>
      </div>
    );
  }
  if (!config) return <div className="center-screen muted">Lädt …</div>;

  if (!config.setup_done) {
    return (
      <Wizard
        initial={config}
        onDone={(c, scanNow) => {
          setConfig(c);
          setAutoScan(scanNow);
          setPage("dashboard");
        }}
      />
    );
  }

  const goOrganize = (target: string) => {
    setOrganizeTarget(target);
    setPage("organize");
  };

  return (
    <div className="layout">
      <aside className="sidebar">
        <div className="brand">
          <img src="/logo.png" alt="" />
          <span>DriveSage</span>
        </div>
        <nav>
          {NAV.map((n) => (
            <button key={n.id} className={page === n.id ? "nav active" : "nav"} onClick={() => setPage(n.id)}>
              <n.icon size={18} />
              {n.label}
            </button>
          ))}
        </nav>
        <div className="sidebar-foot">
          <div className="ai-pill">
            KI: <b>{providerLabel(config)}</b>
          </div>
          <button className="ghost small" onClick={() => api.post("/open-browser").catch(() => {})}>
            <ExternalLink size={14} /> Im Browser öffnen
          </button>
        </div>
      </aside>
      <main className="content">
        {page === "dashboard" && <Dashboard config={config} autoScan={autoScan} onScanned={() => setAutoScan(false)} onOrganize={goOrganize} />}
        {page === "explorer" && <Explorer onOrganize={goOrganize} />}
        {page === "organize" && <Organize config={config} initialTarget={organizeTarget} onOpenSettings={() => setPage("settings")} />}
        {page === "history" && <History />}
        {page === "settings" && <Settings config={config} onSaved={setConfig} onRerunWizard={() => setConfig({ ...config, setup_done: false })} />}
      </main>
    </div>
  );
}

export function providerLabel(c: AppConfig): string {
  switch (c.ai.provider) {
    case "ollama":
      return `Ollama · ${c.ai.model}`;
    case "open_ai":
      return `OpenAI · ${c.ai.model}`;
    case "anthropic":
      return `Claude · ${c.ai.model}`;
    default:
      return "aus (nur Regeln)";
  }
}
