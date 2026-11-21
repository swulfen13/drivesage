import { useEffect, useState } from "react";
import { ArrowLeft, ArrowRight, FolderPlus, ShieldCheck, Undo2, Eye, Trash2, Rocket } from "lucide-react";
import { api, type AppConfig } from "../api";
import AiSettings from "./AiSettings";
import { ErrorBox, FolderPicker } from "./common";

const STEPS = ["Willkommen", "KI", "Ordner", "Los geht's"];

export default function Wizard({ initial, onDone }: { initial: AppConfig; onDone: (c: AppConfig, scanNow: boolean) => void }) {
  const [step, setStep] = useState(0);
  const [cfg, setCfg] = useState<AppConfig>(initial);
  const [suggested, setSuggested] = useState<string[]>([]);
  const [picking, setPicking] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);

  useEffect(() => {
    api.get<{ roots: string[] }>("/fs/suggest").then((r) => setSuggested(r.roots)).catch(() => {});
  }, []);

  const allRoots = Array.from(new Set([...suggested, ...cfg.scan_roots]));
  const toggleRoot = (r: string) =>
    setCfg({ ...cfg, scan_roots: cfg.scan_roots.includes(r) ? cfg.scan_roots.filter((x) => x !== r) : [...cfg.scan_roots, r] });

  const aiReady = cfg.ai.provider === "none" || (cfg.ai.model.trim() !== "" && (cfg.ai.provider === "ollama" || !!cfg.ai.api_key));

  const finish = async (scanNow: boolean) => {
    setSaving(true);
    try {
      const saved = await api.put<AppConfig>("/config", { ...cfg, setup_done: true });
      onDone(saved, scanNow);
    } catch (e) {
      setError((e as Error).message);
      setSaving(false);
    }
  };

  return (
    <div className="wizard">
      <div className="wizard-card">
        <div className="wizard-head">
          <img src="/logo.png" alt="" width={40} />
          <div>
            <h2>DriveSage einrichten</h2>
            <div className="steps">
              {STEPS.map((s, i) => (
                <span key={s} className={i === step ? "step active" : i < step ? "step done" : "step"}>
                  {i + 1}. {s}
                </span>
              ))}
            </div>
          </div>
        </div>

        <div className="wizard-body">
          {step === 0 && (
            <div>
              <h3>Speicher verstehen. Ordnung schaffen.</h3>
              <p className="muted">
                DriveSage zeigt dir, was wie viel Platz belegt, misst, wie schnell deine Laufwerke wirklich sind, und schlägt mit Regeln oder KI eine
                saubere Ordnerstruktur vor – inklusive „alte große Dateien von der schnellen SSD auf die langsame HDD“.
              </p>
              <div className="promise-grid">
                <div className="promise">
                  <Eye size={20} />
                  <b>Du entscheidest</b>
                  <span className="small muted">Jeder Vorschlag wird dir vorher gezeigt. Du kannst jede Datei abwählen oder das Ziel ändern.</span>
                </div>
                <div className="promise">
                  <Trash2 size={20} />
                  <b>Nichts wird gelöscht</b>
                  <span className="small muted">DriveSage verschiebt nur. Duplikate werden angezeigt, aber nie automatisch entfernt.</span>
                </div>
                <div className="promise">
                  <Undo2 size={20} />
                  <b>Alles rückgängig</b>
                  <span className="small muted">Jeder Lauf wird protokolliert und kann mit einem Klick zurückgedreht werden.</span>
                </div>
                <div className="promise">
                  <ShieldCheck size={20} />
                  <b>System bleibt tabu</b>
                  <span className="small muted">Windows, Programme, AppData und Code-Projekte (.git, package.json …) werden nie angefasst.</span>
                </div>
              </div>
            </div>
          )}

          {step === 1 && (
            <div>
              <h3>Womit soll sortiert werden?</h3>
              <p className="muted">Du kannst das später jederzeit in den Einstellungen ändern.</p>
              <AiSettings value={cfg.ai} onChange={(ai) => setCfg({ ...cfg, ai })} />
            </div>
          )}

          {step === 2 && (
            <div>
              <h3>Welche Ordner soll DriveSage anschauen?</h3>
              <p className="muted">Typischerweise Downloads, Desktop und Dokumente. Ganze Laufwerke gehen auch – der Scan dauert dann länger.</p>
              <div className="root-list">
                {allRoots.map((r) => (
                  <label key={r} className="check root">
                    <input type="checkbox" checked={cfg.scan_roots.includes(r)} onChange={() => toggleRoot(r)} />
                    <span className="mono">{r}</span>
                  </label>
                ))}
              </div>
              <button className="ghost" onClick={() => setPicking(true)}>
                <FolderPlus size={16} /> Weiteren Ordner hinzufügen
              </button>
              {picking && (
                <FolderPicker
                  onClose={() => setPicking(false)}
                  onPick={(p) => {
                    setPicking(false);
                    if (!cfg.scan_roots.includes(p)) setCfg({ ...cfg, scan_roots: [...cfg.scan_roots, p] });
                  }}
                />
              )}
            </div>
          )}

          {step === 3 && (
            <div>
              <h3>Fertig eingerichtet 🎉</h3>
              <ul className="summary-list">
                <li>
                  KI: <b>{cfg.ai.provider === "none" ? "keine (nur Regeln)" : `${cfg.ai.provider} · ${cfg.ai.model}`}</b>
                </li>
                <li>
                  Ordner: <b>{cfg.scan_roots.length}</b> ausgewählt
                </li>
                <li>Als „alt“ gelten Dateien, die {cfg.old_after_days} Tage nicht benutzt wurden.</li>
              </ul>
              <p className="muted">Als Nächstes scannt DriveSage deine Ordner. Danach kannst du Laufwerke testen und aufräumen.</p>
            </div>
          )}
          <ErrorBox error={error} onClose={() => setError(null)} />
        </div>

        <div className="wizard-foot">
          <button className="ghost" disabled={step === 0} onClick={() => setStep(step - 1)}>
            <ArrowLeft size={16} /> Zurück
          </button>
          {step < 3 && (
            <button className="primary" disabled={(step === 1 && !aiReady) || (step === 2 && cfg.scan_roots.length === 0)} onClick={() => setStep(step + 1)}>
              Weiter <ArrowRight size={16} />
            </button>
          )}
          {step === 3 && (
            <div className="row">
              <button className="ghost" disabled={saving} onClick={() => finish(false)}>
                Später scannen
              </button>
              <button className="primary" disabled={saving} onClick={() => finish(true)}>
                <Rocket size={16} /> Jetzt scannen
              </button>
            </div>
          )}
        </div>
      </div>
    </div>
  );
}
