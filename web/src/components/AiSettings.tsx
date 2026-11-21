import { useEffect, useState } from "react";
import { Bot, Check, Cloud, Cpu, Download, RefreshCw, Ruler } from "lucide-react";
import { api, waitJob, type AiConfig, type Job, type Provider } from "../api";
import { ErrorBox, JobLine } from "./common";
import { fmtBytes } from "../util";

const PROVIDERS: { id: Provider; title: string; text: string; icon: typeof Bot; badge?: string }[] = [
  { id: "ollama", title: "Lokal mit Ollama", text: "Läuft komplett auf deinem PC. Nichts verlässt den Rechner.", icon: Cpu, badge: "Privat" },
  { id: "anthropic", title: "Claude (Anthropic)", text: "Sehr gute Vorschläge. Braucht einen API-Key.", icon: Cloud },
  { id: "open_ai", title: "OpenAI / kompatibel", text: "OpenAI oder jeder OpenAI-kompatible Server.", icon: Cloud },
  { id: "none", title: "Ohne KI", text: "Nur feste Regeln: nach Typ sortieren, alte große Dateien, Duplikate.", icon: Ruler },
];

const RECOMMENDED_OLLAMA = [
  { name: "qwen2.5:7b", note: "Beste Qualität · ca. 4,7 GB · ab 8 GB VRAM/16 GB RAM" },
  { name: "llama3.2:3b", note: "Schnell & klein · ca. 2 GB · läuft fast überall" },
  { name: "gemma3:4b", note: "Guter Mittelweg · ca. 3,3 GB" },
];

const DEFAULT_MODEL: Record<Provider, string> = {
  none: "",
  ollama: "qwen2.5:7b",
  anthropic: "claude-haiku-5-5",
  open_ai: "gpt-5-mini",
};

interface OllamaStatus {
  running: boolean;
  models: { name: string; size: number }[];
  error?: string;
}

export default function AiSettings({ value, onChange }: { value: AiConfig; onChange: (v: AiConfig) => void }) {
  const [ollama, setOllama] = useState<OllamaStatus | null>(null);
  const [pullJob, setPullJob] = useState<Job | null>(null);
  const [test, setTest] = useState<{ ok: boolean; text: string } | null>(null);
  const [testing, setTesting] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const set = (patch: Partial<AiConfig>) => {
    setTest(null);
    onChange({ ...value, ...patch });
  };

  const refreshOllama = () => {
    const q = value.base_url && value.provider === "ollama" ? `?base_url=${encodeURIComponent(value.base_url)}` : "";
    api.get<OllamaStatus>(`/ai/ollama/models${q}`).then(setOllama).catch(() => setOllama({ running: false, models: [] }));
  };

  useEffect(() => {
    if (value.provider === "ollama") refreshOllama();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [value.provider]);

  const chooseProvider = (p: Provider) => {
    setTest(null);
    onChange({
      ...value,
      provider: p,
      model: p === value.provider ? value.model : DEFAULT_MODEL[p],
      base_url: p === value.provider ? value.base_url : null,
      send_snippets: p === "ollama", // local: snippets on, cloud: off by default
    });
  };

  const pull = async (model: string) => {
    setError(null);
    try {
      const { job_id } = await api.post<{ job_id: string }>("/ai/ollama/pull", { model, base_url: value.base_url });
      await waitJob(job_id, setPullJob);
      setPullJob(null);
      refreshOllama();
      set({ model });
    } catch (e) {
      setPullJob(null);
      setError((e as Error).message);
    }
  };

  const runTest = async () => {
    setTesting(true);
    setTest(null);
    try {
      const r = await api.post<{ reply: string }>("/ai/test", { ai: value });
      setTest({ ok: true, text: `Verbunden – Antwort: „${r.reply}“` });
    } catch (e) {
      setTest({ ok: false, text: (e as Error).message });
    } finally {
      setTesting(false);
    }
  };

  const installed = new Set(ollama?.models.map((m) => m.name) ?? []);

  return (
    <div className="ai-settings">
      <div className="provider-grid">
        {PROVIDERS.map((p) => (
          <button key={p.id} className={value.provider === p.id ? "provider selected" : "provider"} onClick={() => chooseProvider(p.id)}>
            <p.icon size={22} />
            <div className="provider-title">
              {p.title} {p.badge && <span className="badge good">{p.badge}</span>}
            </div>
            <div className="small muted">{p.text}</div>
          </button>
        ))}
      </div>

      {value.provider === "ollama" && (
        <div className="panel">
          <div className="row between">
            <h4>Ollama</h4>
            <button className="ghost small" onClick={refreshOllama}>
              <RefreshCw size={14} /> Neu prüfen
            </button>
          </div>
          {ollama && !ollama.running && (
            <div className="hint">
              Ollama läuft nicht. Installiere es von <a href="https://ollama.com/download" target="_blank" rel="noreferrer">ollama.com/download</a>,
              starte es und klicke auf „Neu prüfen“.
            </div>
          )}
          {ollama?.running && (
            <>
              <label className="field">
                <span>Installiertes Modell</span>
                <select value={installed.has(value.model) ? value.model : ""} onChange={(e) => set({ model: e.target.value })}>
                  <option value="" disabled>
                    {ollama.models.length ? "Modell wählen …" : "Noch kein Modell installiert"}
                  </option>
                  {ollama.models.map((m) => (
                    <option key={m.name} value={m.name}>
                      {m.name} ({fmtBytes(m.size)})
                    </option>
                  ))}
                </select>
              </label>
              <div className="small muted" style={{ margin: "12px 0 6px" }}>
                Empfohlene Modelle:
              </div>
              <div className="model-list">
                {RECOMMENDED_OLLAMA.map((m) => (
                  <div key={m.name} className="model-row">
                    <div className="grow">
                      <div className="mono">{m.name}</div>
                      <div className="small muted">{m.note}</div>
                    </div>
                    {installed.has(m.name) ? (
                      <button className={value.model === m.name ? "small primary" : "small ghost"} onClick={() => set({ model: m.name })}>
                        {value.model === m.name ? <Check size={14} /> : null} {value.model === m.name ? "Ausgewählt" : "Auswählen"}
                      </button>
                    ) : (
                      <button className="small ghost" disabled={!!pullJob} onClick={() => pull(m.name)}>
                        <Download size={14} /> Herunterladen
                      </button>
                    )}
                  </div>
                ))}
              </div>
              <JobLine job={pullJob} />
            </>
          )}
          <details className="small">
            <summary>Erweitert</summary>
            <label className="field">
              <span>Ollama-Adresse</span>
              <input placeholder="http://127.0.0.1:11434" value={value.base_url ?? ""} onChange={(e) => set({ base_url: e.target.value || null })} />
            </label>
            <label className="field">
              <span>Modellname (manuell)</span>
              <input value={value.model} onChange={(e) => set({ model: e.target.value })} />
            </label>
          </details>
        </div>
      )}

      {(value.provider === "anthropic" || value.provider === "open_ai") && (
        <div className="panel">
          <label className="field">
            <span>API-Key</span>
            <input
              type="password"
              placeholder={value.provider === "anthropic" ? "sk-ant-…" : "sk-…"}
              value={value.api_key ?? ""}
              onChange={(e) => set({ api_key: e.target.value })}
            />
            <span className="small muted">Wird nur lokal in deiner DriveSage-Konfiguration gespeichert.</span>
          </label>
          <label className="field">
            <span>Modell</span>
            <input value={value.model} onChange={(e) => set({ model: e.target.value })} />
          </label>
          {value.provider === "open_ai" && (
            <label className="field">
              <span>Basis-URL (optional, für kompatible Server wie LM Studio, Groq, OpenRouter)</span>
              <input placeholder="https://api.openai.com/v1" value={value.base_url ?? ""} onChange={(e) => set({ base_url: e.target.value || null })} />
            </label>
          )}
        </div>
      )}

      {value.provider !== "none" && (
        <div className="panel">
          <label className="check">
            <input type="checkbox" checked={value.send_snippets} onChange={(e) => set({ send_snippets: e.target.checked })} />
            <span>
              Kurze Textvorschau senden (erste ~240 Zeichen von .txt/.md/.csv-Dateien)
              <div className="small muted">
                Bessere Sortierung, aber Inhalte gehen an das Modell.{" "}
                {value.provider === "ollama" ? "Bei Ollama bleibt alles lokal." : "Bei Cloud-Anbietern standardmäßig aus."} Sonst werden nur Dateinamen,
                Größen und Datum gesendet.
              </div>
            </span>
          </label>
          <div className="row" style={{ marginTop: 12 }}>
            <button className="ghost" onClick={runTest} disabled={testing || !value.model}>
              <Bot size={16} /> {testing ? "Teste …" : "Verbindung testen"}
            </button>
            {test && <span className={test.ok ? "good-text small" : "bad-text small"}>{test.text}</span>}
          </div>
        </div>
      )}
      <ErrorBox error={error} onClose={() => setError(null)} />
    </div>
  );
}
