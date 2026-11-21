import { useEffect, useState } from "react";
import { ChevronRight, File, Folder, FolderOpen, Home, Lock, Sparkles } from "lucide-react";
import { api, type TreeNode } from "../api";
import { Bar, ErrorBox } from "./common";
import { CATEGORY, baseName, fmtBytes, fmtDate, fmtNum } from "../util";

export default function Explorer({ onOrganize }: { onOrganize: (target: string) => void }) {
  const [stack, setStack] = useState<string[]>([""]);
  const [node, setNode] = useState<TreeNode | null>(null);
  const [error, setError] = useState<string | null>(null);
  const path = stack[stack.length - 1];

  useEffect(() => {
    api
      .get<TreeNode>(`/scan/tree?path=${encodeURIComponent(path)}`)
      .then((n) => {
        setNode(n);
        setError(null);
      })
      .catch((e) => {
        setNode(null);
        setError(e.message);
      });
  }, [path]);

  const max = Math.max(node?.dirs[0]?.size ?? 0, node?.file_entries[0]?.size ?? 0, 1);

  return (
    <div className="page">
      <div className="page-head">
        <div>
          <h1>Explorer</h1>
          <p className="muted">Klick dich durch deine Ordner – sortiert nach Größe.</p>
        </div>
        {path && node && !node.protected && (
          <div className="row">
            <button className="ghost" onClick={() => api.post("/open-path", { path })}>
              <FolderOpen size={16} /> Im Dateimanager öffnen
            </button>
            <button className="primary" onClick={() => onOrganize(path)}>
              <Sparkles size={16} /> Diesen Ordner aufräumen
            </button>
          </div>
        )}
      </div>

      <div className="crumbs">
        {stack.map((p, i) => (
          <span key={p + i} className="row">
            {i > 0 && <ChevronRight size={14} className="muted" />}
            <button className="crumb" onClick={() => setStack(stack.slice(0, i + 1))}>
              {i === 0 ? <Home size={14} /> : baseName(p)}
            </button>
          </span>
        ))}
      </div>

      <ErrorBox error={error === "Noch kein Scan vorhanden" ? "Noch kein Scan vorhanden – starte zuerst einen Scan in der Übersicht." : error} />

      {node && (
        <div className="card">
          <div className="row between">
            <span className="muted small">
              {fmtBytes(node.size)} · {fmtNum(node.files)} Dateien
            </span>
            {node.protected && (
              <span className="badge">
                <Lock size={12} /> geschützt
              </span>
            )}
          </div>
          <div className="list">
            {node.dirs.map((d) => (
              <button key={d.path} className="list-row clickable" onClick={() => setStack([...stack, d.path])}>
                <Folder size={18} className="folder-icon" />
                <div className="grow ellipsis">
                  <div className="row">
                    {path ? baseName(d.path) : d.path} {d.protected && <Lock size={12} className="muted" />}
                  </div>
                  <Bar value={d.size} max={max} />
                </div>
                <span className="small muted nowrap">{fmtNum(d.files)} Dateien</span>
                <b className="nowrap size-col">{fmtBytes(d.size)}</b>
              </button>
            ))}
            {node.file_entries.map((f) => (
              <div key={f.path} className="list-row">
                <File size={18} style={{ color: CATEGORY[f.category].color }} />
                <div className="grow ellipsis">
                  {baseName(f.path)}
                  <Bar value={f.size} max={max} color={CATEGORY[f.category].color} />
                </div>
                <span className="small muted nowrap">{fmtDate(f.modified)}</span>
                <b className="nowrap size-col">{fmtBytes(f.size)}</b>
              </div>
            ))}
            {node.dirs.length === 0 && node.file_entries.length === 0 && <div className="muted pad">Leer</div>}
          </div>
        </div>
      )}
    </div>
  );
}
