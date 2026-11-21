import { useMemo, useState } from "react";
import { ArrowRight, Bot, CheckCircle2, FlaskConical, FolderOpen, Play, Ruler, Sparkles, Undo2, XCircle, AlertTriangle } from "lucide-react";
import { api, waitJob, type AppConfig, type Job, type Plan, type RunReport } from "../api";
import { ErrorBox, FolderPicker, JobLine } from "./common";
import { baseName, fmtBytes, rel, sep } from "../util";
import { providerLabel } from "../App";

type Mode = "ai" | "rules";

export default function Organize({ config, initialTarget, onOpenSettings }: { config: AppConfig; initialTarget: string | null; onOpenSettings: () => void }) {
  const aiAvailable = config.ai.provider !== "none";
  const [target, setTarget] = useState<string>(initialTarget ?? config.scan_roots[0] ?? "");
  const [mode, setMode] = useState<Mode>(aiAvailable ? "ai" : "rules");
  const [opts, setOpts] = useState({ sort_by_type: true, tier_to_slow_drive: true, report_duplicates: true });
  const [plan, setPlan] = useState<Plan | null>(null);
  const [job, setJob] = useState<Job | null>(null);
  const [busy, setBusy] = useState(false);
  const [picking, setPicking] = useState(false);
  const [report, setReport] = useState<RunReport | null>(null);
  const [confirm, setConfirm] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const create = async () => {
    setError(null);
    setPlan(null);
    setReport(null);
    setBusy(true);
    try {
      if (mode === "rules") {
        setPlan(await api.post<Plan>("/plan/rules", { target, options: { ...opts, old_after_days: config.old_after_days, large_file_mb: config.large_file_mb } }));
      } else {
        const { job_id } = await api.post<{ job_id: string }>("/plan/ai", { target });
        setPlan(await waitJob<Plan>(job_id, setJob));
      }
    } catch (e) {
      setError((e as Error).message);
    } finally {
      setBusy(false);
      setJob(null);
    }
  };

  const patch = async (actions: { id: number; enabled?: boolean; to?: string }[]) => {
    if (!plan) return;
    try {
      setPlan(await api.put<Plan>(`/plans/${plan.id}`, { actions }));
      setReport(null);
    } catch (e) {
      setError((e as Error).message);
    }
  };

  const run = async (dry: boolean) => {
    if (!plan) return;
    setConfirm(false);
    setBusy(true);
    setError(null);
    try {
      setReport(await api.post<RunReport>(`/plans/${plan.id}/execute`, { dry_run: dry }));
    } catch (e) {
      setError((e as Error).message);
    } finally {
      setBusy(false);
    }
  };

  const undo = async () => {
    if (!report) return;
    setBusy(true);
    try {
      const u = await api.post<RunReport>(`/runs/${report.run_id}/undo`);
      setReport({ ...report, undone: true });
      setError(u.failed > 0 ? `${u.failed} Dateien konnten nicht zurückverschoben werden.` : null);
    } catch (e) {
      setError((e as Error).message);
    } finally {
      setBusy(false);
    }
  };

  const enabled = plan?.actions.filter((a) => a.enabled) ?? [];
  const enabledBytes = enabled.reduce((s, a) => s + a.size, 0);

  // group by destination folder for a readable "after" view
  const groups = useMemo(() => {
    if (!plan) return [];
    const m = new Map<string, typeof plan.actions>();
    for (const a of plan.actions) {
      const s = sep(a.to);
      const dir = a.to.slice(0, a.to.lastIndexOf(s));
      const key = dir.startsWith(plan.target_root) ? rel(dir, plan.target_root) : dir;
      m.set(key, [...(m.get(key) ?? []), a]);
    }
    return [...m.entries()].sort((a, b) => a[0].localeCompare(b[0]));
  }, [plan]);

  const executed = report && !report.dry_run;

  return (
    <div className="page">
      <div className="page-head">
        <div>
          <h1>Aufräumen</h1>
          <p className="muted">Vorschlag erstellen, prüfen, anpassen – erst dann wird etwas verschoben.</p>
        </div>
      </div>

      <div className="card">
        <div className="organize-form">
          <label className="field grow">
            <span>Ordner</span>
            <div className="row">
              <select value={target} onChange={(e) => setTarget(e.target.value)} className="grow">
                {!config.scan_roots.includes(target) && target && <option value={target}>{target}</option>}
                {config.scan_roots.map((r) => (
                  <option key={r} value={r}>
                    {r}
                  </option>
                ))}
              </select>
              <button className="ghost" onClick={() => setPicking(true)}>
                <FolderOpen size={16} /> Anderer …
              </button>
            </div>
          </label>
          <div className="field">
            <span>Methode</span>
            <div className="segmented">
              <button className={mode === "ai" ? "active" : ""} disabled={!aiAvailable} onClick={() => setMode("ai")} title={aiAvailable ? providerLabel(config) : "Keine KI eingerichtet"}>
                <Bot size={16} /> KI
              </button>
              <button className={mode === "rules" ? "active" : ""} onClick={() => setMode("rules")}>
                <Ruler size={16} /> Regeln
              </button>
            </div>
          </div>
        </div>
        {mode === "rules" && (
          <div className="row wrap" style={{ gap: 18, marginTop: 12 }}>
            <label className="check">
              <input type="checkbox" checked={opts.sort_by_type} onChange={(e) => setOpts({ ...opts, sort_by_type: e.target.checked })} />
              <span>Lose Dateien nach Typ sortieren</span>
            </label>
            <label className="check">
              <input type="checkbox" checked={opts.tier_to_slow_drive} onChange={(e) => setOpts({ ...opts, tier_to_slow_drive: e.target.checked })} />
              <span>
                Große, alte Dateien (&gt; {config.large_file_mb} MB, {config.old_after_days}+ Tage) auf langsameres Laufwerk
              </span>
            </label>
            <label className="check">
              <input type="checkbox" checked={opts.report_duplicates} onChange={(e) => setOpts({ ...opts, report_duplicates: e.target.checked })} />
              <span>Duplikate anzeigen</span>
            </label>
          </div>
        )}
        {mode === "ai" && (
          <p className="small muted" style={{ marginTop: 10 }}>
            {providerLabel(config)} · {config.ai.send_snippets ? "mit Textvorschau" : "nur Dateinamen, Größen & Datum"} ·{" "}
            <button className="link" onClick={onOpenSettings}>
              ändern
            </button>
          </p>
        )}
        {!aiAvailable && (
          <p className="small muted" style={{ marginTop: 10 }}>
            Tipp: Mit KI werden Dateien auch nach Inhalt/Thema sortiert (z. B. „Rechnungen/2024“).{" "}
            <button className="link" onClick={onOpenSettings}>
              KI einrichten
            </button>
          </p>
        )}
        <div className="row" style={{ marginTop: 14 }}>
          <button className="primary" disabled={!target || busy} onClick={create}>
            <Sparkles size={16} /> Vorschlag erstellen
          </button>
        </div>
        <JobLine job={job} />
      </div>

      {picking && (
        <FolderPicker
          onClose={() => setPicking(false)}
          onPick={(p) => {
            setTarget(p);
            setPicking(false);
          }}
        />
      )}
      <ErrorBox error={error} onClose={() => setError(null)} />

      {plan && (
        <section>
          <div className="card plan-head">
            <div>
              <h3>{plan.title}</h3>
              <p className="muted">{plan.summary}</p>
              <div className="small muted">
                Quelle: {plan.source === "rules" ? "Regeln" : plan.source.replace("ai:", "KI · ")}
              </div>
            </div>
            <div className="plan-stats">
              <b>{enabled.length}</b>
              <span className="small muted">von {plan.actions.length} Verschiebungen</span>
              <span className="small muted">{fmtBytes(enabledBytes)}</span>
            </div>
          </div>

          {plan.actions.length > 0 && (
            <div className="card">
              <div className="row between">
                <div className="row">
                  <button className="ghost small" onClick={() => patch(plan.actions.map((a) => ({ id: a.id, enabled: true })))}>
                    Alle an
                  </button>
                  <button className="ghost small" onClick={() => patch(plan.actions.map((a) => ({ id: a.id, enabled: false })))}>
                    Alle aus
                  </button>
                </div>
                <span className="small muted">Ziel anklicken, um es zu ändern</span>
              </div>
              {groups.map(([folder, actions]) => (
                <div key={folder} className="plan-group">
                  <div className="plan-group-head">
                    <FolderOpen size={16} /> <span className="mono">{folder || "."}</span>
                    <span className="small muted">{actions.length} Dateien</span>
                  </div>
                  {actions.map((a) => (
                    <div key={a.id} className={a.enabled ? "action" : "action disabled"}>
                      <input type="checkbox" checked={a.enabled} onChange={(e) => patch([{ id: a.id, enabled: e.target.checked }])} />
                      <div className="action-from ellipsis" title={a.from}>
                        {rel(a.from, plan.target_root)}
                      </div>
                      <ArrowRight size={14} className="muted" />
                      <EditableTarget value={a.to} root={plan.target_root} onSave={(to) => patch([{ id: a.id, to }])} />
                      <span className="small muted nowrap">{fmtBytes(a.size)}</span>
                      <span className="reason small" title={a.reason}>
                        {a.reason}
                      </span>
                    </div>
                  ))}
                </div>
              ))}
            </div>
          )}

          {plan.findings.length > 0 && (
            <div className="card">
              <h3>Hinweise (keine automatische Aktion)</h3>
              {plan.findings.map((f, i) => (
                <details key={i} className="finding">
                  <summary>
                    <b>{f.title}</b> – {f.detail}
                  </summary>
                  <ul className="mono small">
                    {f.paths.map((p) => (
                      <li key={p}>{p}</li>
                    ))}
                  </ul>
                </details>
              ))}
            </div>
          )}

          {plan.actions.length > 0 && !executed && (
            <div className="card action-bar">
              <button className="ghost" disabled={busy || enabled.length === 0} onClick={() => run(true)}>
                <FlaskConical size={16} /> Testlauf
              </button>
              <button className="primary" disabled={busy || enabled.length === 0} onClick={() => setConfirm(true)}>
                <Play size={16} /> {enabled.length} Dateien verschieben
              </button>
            </div>
          )}

          {report && (
            <div className="card">
              <div className="row between">
                <h3>{report.dry_run ? "Ergebnis Testlauf (nichts wurde verändert)" : report.undone ? "Rückgängig gemacht" : "Erledigt"}</h3>
                {executed && !report.undone && (
                  <button className="ghost" disabled={busy} onClick={undo}>
                    <Undo2 size={16} /> Rückgängig machen
                  </button>
                )}
              </div>
              <div className="row" style={{ gap: 20 }}>
                <span className="good-text">
                  <CheckCircle2 size={14} /> {report.moved} {report.dry_run ? "würden verschoben" : "verschoben"} ({fmtBytes(report.bytes_moved)})
                </span>
                {report.skipped > 0 && (
                  <span className="warn-text">
                    <AlertTriangle size={14} /> {report.skipped} übersprungen
                  </span>
                )}
                {report.failed > 0 && (
                  <span className="bad-text">
                    <XCircle size={14} /> {report.failed} fehlgeschlagen
                  </span>
                )}
              </div>
              {report.steps.filter((s) => s.status !== "ok" || s.message !== (report.dry_run ? "würde verschoben" : "verschoben")).length > 0 && (
                <ul className="small step-list">
                  {report.steps
                    .filter((s) => s.status !== "ok" || s.message !== (report.dry_run ? "würde verschoben" : "verschoben"))
                    .map((s) => (
                      <li key={s.action_id} className={`step-${s.status}`}>
                        <span className="mono">{baseName(s.from)}</span>: {s.message}
                      </li>
                    ))}
                </ul>
              )}
              {executed && <p className="small muted">Der Scan ist jetzt veraltet – starte in der Übersicht einen neuen Scan.</p>}
            </div>
          )}
        </section>
      )}

      {confirm && plan && (
        <div className="modal-bg" onClick={() => setConfirm(false)}>
          <div className="modal" onClick={(e) => e.stopPropagation()}>
            <h3>{enabled.length} Dateien verschieben?</h3>
            <p className="muted">
              {fmtBytes(enabledBytes)} werden verschoben. Es wird nichts gelöscht, und du kannst den Lauf danach im Verlauf rückgängig machen.
            </p>
            <div className="modal-foot">
              <button className="ghost" onClick={() => setConfirm(false)}>
                Abbrechen
              </button>
              <button className="primary" onClick={() => run(false)}>
                <Play size={16} /> Jetzt verschieben
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}

function EditableTarget({ value, root, onSave }: { value: string; root: string; onSave: (v: string) => void }) {
  const [editing, setEditing] = useState(false);
  const inside = value.startsWith(root);
  const shown = inside ? rel(value, root) : value;
  const [draft, setDraft] = useState(shown);
  if (!editing) {
    return (
      <button className="action-to ellipsis" title={value} onClick={() => { setDraft(shown); setEditing(true); }}>
        {shown}
      </button>
    );
  }
  const commit = () => {
    setEditing(false);
    const d = draft.trim();
    if (!d || d === shown) return;
    const abs = inside && !/^([a-zA-Z]:|[\\/])/.test(d) ? root.replace(/[\\/]$/, "") + sep(root) + d : d;
    onSave(abs);
  };
  return (
    <input
      className="action-to-input"
      autoFocus
      value={draft}
      onChange={(e) => setDraft(e.target.value)}
      onBlur={commit}
      onKeyDown={(e) => {
        if (e.key === "Enter") commit();
        if (e.key === "Escape") setEditing(false);
      }}
    />
  );
}
