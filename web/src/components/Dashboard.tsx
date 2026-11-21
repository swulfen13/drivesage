import { useCallback, useEffect, useState } from "react";
import { Copy, Clock, Gauge, HardDrive, Play, RefreshCw, ScanSearch, Sparkles, Square, Usb } from "lucide-react";
import { api, waitJob, type AppConfig, type Drive, type Job, type ScanSummary, type BenchResult } from "../api";
import { Bar, ErrorBox, JobLine } from "./common";
import { CATEGORY, baseName, fmtBytes, fmtNum, speedLabel } from "../util";

export default function Dashboard({
  config,
  autoScan,
  onScanned,
  onOrganize,
}: {
  config: AppConfig;
  autoScan: boolean;
  onScanned: () => void;
  onOrganize: (target: string) => void;
}) {
  const [drives, setDrives] = useState<Drive[] | null>(null);
  const [summary, setSummary] = useState<ScanSummary | null>(null);
  const [scanJob, setScanJob] = useState<Job | null>(null);
  const [benchJobs, setBenchJobs] = useState<Record<string, Job | null>>({});
  const [error, setError] = useState<string | null>(null);

  const loadDrives = useCallback(() => {
    api.get<{ drives: Drive[] }>("/drives").then((d) => setDrives(d.drives)).catch((e) => setError(e.message));
  }, []);

  const startScan = useCallback(async () => {
    setError(null);
    try {
      const { job_id } = await api.post<{ job_id: string }>("/scan", { roots: config.scan_roots });
      const s = await waitJob<ScanSummary>(job_id, setScanJob);
      setSummary(s);
    } catch (e) {
      setError((e as Error).message);
    } finally {
      setScanJob(null);
      onScanned();
    }
  }, [config.scan_roots, onScanned]);

  useEffect(() => {
    loadDrives();
    api.get<ScanSummary | null>("/scan/summary").then(setSummary).catch(() => {});
  }, [loadDrives]);

  useEffect(() => {
    if (autoScan) startScan();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [autoScan]);

  const cancelScan = () => scanJob && api.post(`/jobs/${scanJob.id}/cancel`);

  const bench = async (d: Drive) => {
    setError(null);
    try {
      const { job_id } = await api.post<{ job_id: string }>("/bench", { drive_id: d.id, size_mb: 512 });
      await waitJob<BenchResult>(job_id, (j) => setBenchJobs((b) => ({ ...b, [d.id]: j })));
      loadDrives();
    } catch (e) {
      setError(`${d.mount_point}: ${(e as Error).message}`);
    } finally {
      setBenchJobs((b) => ({ ...b, [d.id]: null }));
    }
  };

  const maxDir = summary?.largest_dirs[0]?.size ?? 1;
  const maxFile = summary?.largest_files[0]?.size ?? 1;

  return (
    <div className="page">
      <div className="page-head">
        <div>
          <h1>Übersicht</h1>
          <p className="muted">Laufwerke, Geschwindigkeit und was deinen Speicher belegt.</p>
        </div>
      </div>
      <ErrorBox error={error} onClose={() => setError(null)} />

      <section>
        <div className="section-head">
          <h2>
            <HardDrive size={18} /> Laufwerke
          </h2>
          <button className="ghost small" onClick={loadDrives}>
            <RefreshCw size={14} /> Aktualisieren
          </button>
        </div>
        <div className="drive-grid">
          {drives === null && <div className="muted">Lädt …</div>}
          {drives?.map((d) => {
            const usedPct = d.total_bytes ? d.used_bytes / d.total_bytes : 0;
            const job = benchJobs[d.id];
            const sl = d.bench ? speedLabel(d.bench.seq_read_mbps) : null;
            return (
              <div key={d.id} className="card drive">
                <div className="row between">
                  <div className="row grow">
                    {d.removable ? <Usb size={20} /> : <HardDrive size={20} />}
                    <div className="grow ellipsis" title={`${d.mount_point} · ${d.name}`}>
                      <div className="drive-title ellipsis">{d.mount_point}</div>
                      <div className="small muted ellipsis">
                        {d.name || "Laufwerk"} · {d.file_system}
                      </div>
                    </div>
                  </div>
                  <span className={`badge kind-${d.kind}`}>{d.kind === "ssd" ? "SSD" : d.kind === "hdd" ? "HDD" : d.removable ? "Extern" : "?"}</span>
                </div>
                <div className={usedPct > 0.9 ? "usage danger" : "usage"}>
                  <div style={{ width: `${usedPct * 100}%` }} />
                </div>
                <div className="row between small">
                  <span>{fmtBytes(d.used_bytes)} belegt</span>
                  <span className="muted">
                    {fmtBytes(d.available_bytes)} frei von {fmtBytes(d.total_bytes)}
                  </span>
                </div>
                {d.bench ? (
                  <div className="bench">
                    <div>
                      <span className="small muted">Lesen</span>
                      <b>{fmtNum(d.bench.seq_read_mbps)} MB/s</b>
                    </div>
                    <div>
                      <span className="small muted">Schreiben</span>
                      <b>{fmtNum(d.bench.seq_write_mbps)} MB/s</b>
                    </div>
                    <div>
                      <span className="small muted">4K zufällig</span>
                      <b>{fmtNum(d.bench.rand4k_read_iops)} IOPS</b>
                    </div>
                  </div>
                ) : (
                  <div className="small muted bench-empty">Noch nicht gemessen</div>
                )}
                {sl && (
                  <div className="row between small">
                    <span className={`tone-${sl.tone}`}>{sl.label}</span>
                    {!d.bench!.cache_bypassed && <span className="muted" title="Das Dateisystem erlaubt keinen Test am Cache vorbei – Werte können zu hoch sein.">Cache aktiv</span>}
                  </div>
                )}
                {job ? (
                  <JobLine job={job} />
                ) : (
                  <button className="ghost small" disabled={d.read_only || Object.values(benchJobs).some(Boolean)} onClick={() => bench(d)}>
                    <Gauge size={14} /> {d.bench ? "Erneut testen" : "Geschwindigkeit testen"}
                  </button>
                )}
              </div>
            );
          })}
        </div>
        <p className="small muted">Der Test schreibt eine 512-MB-Testdatei, liest sie wieder und löscht sie danach. Dauert je nach Laufwerk 5–60 Sekunden.</p>
      </section>

      <section>
        <div className="section-head">
          <h2>
            <ScanSearch size={18} /> Speicheranalyse
          </h2>
          {scanJob ? (
            <button className="ghost small" onClick={cancelScan}>
              <Square size={14} /> Abbrechen
            </button>
          ) : (
            <button className="primary small" onClick={startScan}>
              <Play size={14} /> {summary ? "Neu scannen" : "Scan starten"}
            </button>
          )}
        </div>
        <JobLine job={scanJob} />
        {!summary && !scanJob && (
          <div className="card empty">
            <p>Noch kein Scan. Gescannt werden: {config.scan_roots.map(baseName).join(", ") || "–"}</p>
          </div>
        )}
        {summary && (
          <>
            <div className="stat-grid">
              <div className="card stat">
                <span className="muted small">Gesamt</span>
                <b>{fmtBytes(summary.total_bytes)}</b>
                <span className="small muted">
                  {fmtNum(summary.total_files)} Dateien · {fmtNum(summary.total_dirs)} Ordner
                </span>
              </div>
              <div className="card stat">
                <span className="muted small">
                  <Clock size={12} /> Lange ungenutzt
                </span>
                <b>{fmtBytes(summary.old_bytes)}</b>
                <span className="small muted">
                  {fmtNum(summary.old_files)} Dateien seit {config.old_after_days}+ Tagen
                </span>
              </div>
              <div className="card stat">
                <span className="muted small">
                  <Copy size={12} /> Mögliche Duplikate
                </span>
                <b>{fmtBytes(summary.duplicate_wasted_bytes)}</b>
                <span className="small muted">{summary.duplicates.length} Gruppen</span>
              </div>
              <div className="card stat">
                <span className="muted small">Scan-Dauer</span>
                <b>{(summary.duration_ms / 1000).toFixed(1)} s</b>
                <span className="small muted">
                  {summary.errors > 0 ? `${summary.errors} nicht lesbar` : "ohne Fehler"}
                  {summary.cancelled ? " · abgebrochen" : ""}
                </span>
              </div>
            </div>

            <div className="card">
              <h3>Nach Dateityp</h3>
              <div className="stacked">
                {summary.by_category.map((c) => (
                  <div
                    key={c.category}
                    title={`${CATEGORY[c.category].label}: ${fmtBytes(c.bytes)}`}
                    style={{ width: `${(c.bytes / Math.max(1, summary.total_bytes)) * 100}%`, background: CATEGORY[c.category].color }}
                  />
                ))}
              </div>
              <div className="legend">
                {summary.by_category.map((c) => (
                  <span key={c.category}>
                    <i style={{ background: CATEGORY[c.category].color }} />
                    {CATEGORY[c.category].label} <b>{fmtBytes(c.bytes)}</b> <span className="muted">({fmtNum(c.files)})</span>
                  </span>
                ))}
              </div>
            </div>

            <div className="two-col">
              <div className="card">
                <h3>Größte Ordner</h3>
                <div className="list">
                  {summary.largest_dirs.slice(0, 12).map((d) => (
                    <div key={d.path} className="list-row">
                      <div className="grow ellipsis" title={d.path}>
                        {baseName(d.path)}
                        <div className="small muted ellipsis">{d.path}</div>
                        <Bar value={d.size} max={maxDir} />
                      </div>
                      <b className="nowrap">{fmtBytes(d.size)}</b>
                      {!d.protected && (
                        <button className="icon" title="Diesen Ordner aufräumen" onClick={() => onOrganize(d.path)}>
                          <Sparkles size={14} />
                        </button>
                      )}
                    </div>
                  ))}
                </div>
              </div>
              <div className="card">
                <h3>Größte Dateien</h3>
                <div className="list">
                  {summary.largest_files.slice(0, 12).map((f) => (
                    <div key={f.path} className="list-row">
                      <div className="grow ellipsis" title={f.path}>
                        {baseName(f.path)}
                        <div className="small muted ellipsis">{f.path}</div>
                        <Bar value={f.size} max={maxFile} color={CATEGORY[f.category].color} />
                      </div>
                      <b className="nowrap">{fmtBytes(f.size)}</b>
                    </div>
                  ))}
                </div>
              </div>
            </div>
          </>
        )}
      </section>
    </div>
  );
}
