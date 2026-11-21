import { useState } from "react";
import { FolderPlus, Save, Trash2, Wand2 } from "lucide-react";
import { api, type AppConfig } from "../api";
import AiSettings from "./AiSettings";
import { ErrorBox, FolderPicker } from "./common";

export default function Settings({ config, onSaved, onRerunWizard }: { config: AppConfig; onSaved: (c: AppConfig) => void; onRerunWizard: () => void }) {
  const [cfg, setCfg] = useState(config);
  const [picking, setPicking] = useState<"root" | "exclude" | null>(null);
  const [msg, setMsg] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  const save = async () => {
    try {
      const s = await api.put<AppConfig>("/config", cfg);
      setCfg(s);
      onSaved(s);
      setMsg("Gespeichert ✓");
      setTimeout(() => setMsg(null), 2000);
    } catch (e) {
      setError((e as Error).message);
    }
  };

  const list = (key: "scan_roots" | "excluded", title: string) => (
    <div className="panel">
      <h4>{title}</h4>
      {cfg[key].length === 0 && <div className="small muted">Keine</div>}
      {cfg[key].map((p) => (
        <div key={p} className="row between list-row">
          <span className="mono small">{p}</span>
          <button className="icon" onClick={() => setCfg({ ...cfg, [key]: cfg[key].filter((x) => x !== p) })}>
            <Trash2 size={14} />
          </button>
        </div>
      ))}
      <button className="ghost small" onClick={() => setPicking(key === "scan_roots" ? "root" : "exclude")}>
        <FolderPlus size={14} /> Hinzufügen
      </button>
    </div>
  );

  return (
    <div className="page">
      <div className="page-head">
        <div>
          <h1>Einstellungen</h1>
        </div>
        <div className="row">
          {msg && <span className="good-text small">{msg}</span>}
          <button className="primary" onClick={save}>
            <Save size={16} /> Speichern
          </button>
        </div>
      </div>
      <ErrorBox error={error} onClose={() => setError(null)} />

      <section>
        <h2>KI</h2>
        <AiSettings value={cfg.ai} onChange={(ai) => setCfg({ ...cfg, ai })} />
      </section>

      <section>
        <h2>Ordner</h2>
        <div className="two-col">
          {list("scan_roots", "Gescannte Ordner")}
          {list("excluded", "Ausgeschlossene Ordner")}
        </div>
      </section>

      <section>
        <h2>Regeln</h2>
        <div className="panel two-col">
          <label className="field">
            <span>„Alt“ ab wie vielen Tagen ohne Nutzung?</span>
            <input type="number" min={7} value={cfg.old_after_days} onChange={(e) => setCfg({ ...cfg, old_after_days: Number(e.target.value) })} />
          </label>
          <label className="field">
            <span>„Groß“ ab wie vielen MB?</span>
            <input type="number" min={10} value={cfg.large_file_mb} onChange={(e) => setCfg({ ...cfg, large_file_mb: Number(e.target.value) })} />
          </label>
        </div>
      </section>

      <section>
        <button className="ghost" onClick={onRerunWizard}>
          <Wand2 size={16} /> Einrichtungs-Assistent erneut starten
        </button>
      </section>

      {picking && (
        <FolderPicker
          onClose={() => setPicking(null)}
          onPick={(p) => {
            const key = picking === "root" ? "scan_roots" : "excluded";
            if (!cfg[key].includes(p)) setCfg({ ...cfg, [key]: [...cfg[key], p] });
            setPicking(null);
          }}
        />
      )}
    </div>
  );
}
