import { useCallback, useEffect, useState } from "react";
import { Undo2 } from "lucide-react";
import { api, type RunReport } from "../api";
import { ErrorBox } from "./common";
import { fmtBytes } from "../util";

export default function History() {
  const [runs, setRuns] = useState<RunReport[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState<string | null>(null);

  const load = useCallback(() => {
    api.get<{ runs: RunReport[] }>("/runs").then((r) => setRuns(r.runs)).catch((e) => setError(e.message));
  }, []);
  useEffect(load, [load]);

  const undo = async (id: string) => {
    if (!confirm("Diesen Lauf rückgängig machen? Alle Dateien werden an ihren alten Ort zurückverschoben.")) return;
    setBusy(id);
    try {
      const r = await api.post<RunReport>(`/runs/${id}/undo`);
      if (r.failed > 0) setError(`${r.failed} Dateien konnten nicht zurückverschoben werden (wurden sie inzwischen verändert?).`);
      load();
    } catch (e) {
      setError((e as Error).message);
    } finally {
      setBusy(null);
    }
  };

  return (
    <div className="page">
      <div className="page-head">
        <div>
          <h1>Verlauf</h1>
          <p className="muted">Jeder Aufräum-Lauf wird protokolliert und kann rückgängig gemacht werden.</p>
        </div>
      </div>
      <ErrorBox error={error} onClose={() => setError(null)} />
      <div className="card">
        {runs?.length === 0 && <div className="muted pad">Noch keine Läufe.</div>}
        <div className="list">
          {runs?.map((r) => (
            <div key={r.run_id} className="list-row">
              <div className="grow">
                <div>{r.plan_title}</div>
                <div className="small muted">
                  {new Date(r.started_at).toLocaleString("de-DE")} · {r.moved} verschoben ({fmtBytes(r.bytes_moved)})
                  {r.failed > 0 && ` · ${r.failed} fehlgeschlagen`}
                </div>
              </div>
              {r.undone ? (
                <span className="badge">rückgängig gemacht</span>
              ) : (
                <button className="ghost small" disabled={busy === r.run_id} onClick={() => undo(r.run_id)}>
                  <Undo2 size={14} /> Rückgängig
                </button>
              )}
            </div>
          ))}
        </div>
      </div>
    </div>
  );
}
