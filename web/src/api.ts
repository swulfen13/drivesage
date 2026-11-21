// Thin client for the local DriveSage API.

export type Provider = "none" | "ollama" | "open_ai" | "anthropic";

export interface AiConfig {
  provider: Provider;
  model: string;
  base_url: string | null;
  api_key: string | null;
  send_snippets: boolean;
}

export interface AppConfig {
  setup_done: boolean;
  ai: AiConfig;
  scan_roots: string[];
  excluded: string[];
  old_after_days: number;
  large_file_mb: number;
}

export interface BenchResult {
  seq_write_mbps: number;
  seq_read_mbps: number;
  rand4k_read_iops: number;
  rand4k_read_mbps: number;
  cache_bypassed: boolean;
  size_mb: number;
  duration_ms: number;
}

export interface Drive {
  id: string;
  name: string;
  mount_point: string;
  file_system: string;
  kind: "ssd" | "hdd" | "unknown";
  removable: boolean;
  read_only: boolean;
  total_bytes: number;
  available_bytes: number;
  used_bytes: number;
  bench: BenchResult | null;
  speed_score: number;
}

export type Category = "documents" | "images" | "videos" | "audio" | "archives" | "installers" | "code" | "other";

export interface FileEntry {
  path: string;
  size: number;
  modified: number | null;
  accessed: number | null;
  category: Category;
  protected: boolean;
}

export interface DirEntry {
  path: string;
  size: number;
  files: number;
  protected: boolean;
}

export interface ScanSummary {
  roots: string[];
  total_bytes: number;
  total_files: number;
  total_dirs: number;
  by_category: { category: Category; bytes: number; files: number }[];
  largest_files: FileEntry[];
  largest_dirs: DirEntry[];
  old_bytes: number;
  old_files: number;
  duplicates: { size: number; paths: string[]; wasted: number }[];
  duplicate_wasted_bytes: number;
  errors: number;
  duration_ms: number;
  cancelled: boolean;
}

export interface TreeNode {
  path: string;
  size: number;
  files: number;
  protected: boolean;
  dirs: DirEntry[];
  file_entries: FileEntry[];
}

export interface Action {
  id: number;
  from: string;
  to: string;
  size: number;
  reason: string;
  rule: string;
  enabled: boolean;
}

export interface Plan {
  id: string;
  created_at: string;
  title: string;
  source: string;
  target_root: string;
  actions: Action[];
  findings: { kind: string; title: string; detail: string; bytes: number; paths: string[] }[];
  summary: string;
}

export interface StepResult {
  action_id: number;
  from: string;
  to: string;
  status: "ok" | "skipped" | "failed";
  message: string;
}

export interface RunReport {
  run_id: string;
  plan_id: string;
  plan_title: string;
  dry_run: boolean;
  started_at: string;
  finished_at: string;
  moved: number;
  skipped: number;
  failed: number;
  bytes_moved: number;
  steps: StepResult[];
  undone: boolean;
}

export interface Job<T = unknown> {
  id: string;
  kind: string;
  status: "running" | "done" | "failed" | "cancelled";
  progress: number;
  message: string;
  result: T | null;
  error: string | null;
}

function readToken(): string {
  const m = location.hash.match(/token=([a-f0-9]+)/);
  if (m) {
    sessionStorage.setItem("ds_token", m[1]);
    history.replaceState(null, "", location.pathname + location.search);
    return m[1];
  }
  return sessionStorage.getItem("ds_token") ?? "";
}

const token = readToken();
export const hasToken = token.length > 0;

async function req<T>(method: string, path: string, body?: unknown): Promise<T> {
  const res = await fetch(`/api${path}`, {
    method,
    headers: { "content-type": "application/json", "x-drivesage-token": token },
    body: body === undefined ? undefined : JSON.stringify(body),
  });
  const text = await res.text();
  const data = text ? JSON.parse(text) : null;
  if (!res.ok) throw new Error(data?.error ?? text ?? res.statusText);
  return data as T;
}

export const api = {
  get: <T,>(p: string) => req<T>("GET", p),
  post: <T,>(p: string, b: unknown = {}) => req<T>("POST", p, b),
  put: <T,>(p: string, b: unknown) => req<T>("PUT", p, b),
};

/** Poll a background job until it finishes. */
export async function waitJob<T>(jobId: string, onUpdate?: (j: Job<T>) => void): Promise<T> {
  for (;;) {
    const j = await api.get<Job<T>>(`/jobs/${jobId}`);
    onUpdate?.(j);
    if (j.status === "done" || j.status === "cancelled") return j.result as T;
    if (j.status === "failed") throw new Error(j.error ?? "Job fehlgeschlagen");
    await new Promise((r) => setTimeout(r, 400));
  }
}
