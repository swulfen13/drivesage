import { useEffect, useState } from "react";
import { ChevronRight, Folder, FolderUp, HardDrive, Lock, X } from "lucide-react";
import { api, type Job } from "../api";

export function Progress({ value, label }: { value: number; label?: string }) {
  const indeterminate = value < 0;
  return (
    <div className="progress-wrap">
      <div className={indeterminate ? "progress indeterminate" : "progress"}>
        <div style={{ width: indeterminate ? "30%" : `${Math.round(value * 100)}%` }} />
      </div>
      {label && <div className="small muted">{label}</div>}
    </div>
  );
}

export function JobLine({ job }: { job: Job | null }) {
  if (!job || job.status !== "running") return null;
  return <Progress value={job.progress} label={job.message} />;
}

export function Bar({ value, max, color }: { value: number; max: number; color?: string }) {
  const pct = max > 0 ? Math.max(1, (value / max) * 100) : 0;
  return (
    <div className="bar">
      <div style={{ width: `${pct}%`, background: color }} />
    </div>
  );
}

export function ErrorBox({ error, onClose }: { error: string | null; onClose?: () => void }) {
  if (!error) return null;
  return (
    <div className="error-box">
      <span>{error}</span>
      {onClose && (
        <button className="icon" onClick={onClose}>
          <X size={14} />
        </button>
      )}
    </div>
  );
}

interface DirListing {
  path: string;
  parent: string | null;
  dirs: { name: string; path: string; protected?: boolean }[];
}

/** Simple folder browser (the web UI can't open native dialogs). */
export function FolderPicker({ onPick, onClose }: { onPick: (p: string) => void; onClose: () => void }) {
  const [path, setPath] = useState("");
  const [listing, setListing] = useState<DirListing | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    api
      .get<DirListing>(`/fs/dirs?path=${encodeURIComponent(path)}`)
      .then((l) => {
        setListing(l);
        setError(null);
      })
      .catch((e) => setError(e.message));
  }, [path]);

  return (
    <div className="modal-bg" onClick={onClose}>
      <div className="modal" onClick={(e) => e.stopPropagation()}>
        <div className="modal-head">
          <h3>Ordner auswählen</h3>
          <button className="icon" onClick={onClose}>
            <X size={16} />
          </button>
        </div>
        <div className="picker-path mono small">{path || "Start"}</div>
        <ErrorBox error={error} />
        <div className="picker-list">
          {path && (
            <button className="picker-item" onClick={() => setPath(listing?.parent ?? "")}>
              <FolderUp size={16} /> ..
            </button>
          )}
          {listing?.dirs.map((d) => (
            <button key={d.path} className="picker-item" onClick={() => setPath(d.path)} disabled={d.protected}>
              {path ? <Folder size={16} /> : <HardDrive size={16} />}
              <span className="grow">{d.name}</span>
              {d.protected ? <Lock size={14} /> : <ChevronRight size={14} />}
            </button>
          ))}
          {listing && listing.dirs.length === 0 && <div className="muted small pad">Keine Unterordner</div>}
        </div>
        <div className="modal-foot">
          <button className="ghost" onClick={onClose}>
            Abbrechen
          </button>
          <button className="primary" disabled={!path} onClick={() => onPick(path)}>
            Diesen Ordner wählen
          </button>
        </div>
      </div>
    </div>
  );
}
