use crate::jobs::{self, JobCtx};
use crate::Shared;
use anyhow::{anyhow, Context};
use axum::{
    extract::{Path as AxPath, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use drivesage_core::{
    ai::{self, AiClient},
    bench,
    config::{default_scan_roots, AiConfig, AppConfig},
    drives::{self, DriveInfo},
    plan::{self, Plan},
    rules::{self, RuleOptions},
    scan::{self, ScanIndex},
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use uuid::Uuid;

pub struct ApiError(anyhow::Error);
impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": format!("{:#}", self.0)})),
        )
            .into_response()
    }
}
impl<E: Into<anyhow::Error>> From<E> for ApiError {
    fn from(e: E) -> Self {
        ApiError(e.into())
    }
}
type ApiResult<T> = Result<Json<T>, ApiError>;

pub fn routes() -> Router<Shared> {
    Router::new()
        .route(
            "/health",
            get(|| async { Json(json!({"ok": true, "version": env!("CARGO_PKG_VERSION")})) }),
        )
        .route("/config", get(get_config).put(put_config))
        .route("/drives", get(get_drives))
        .route("/bench", post(start_bench))
        .route("/scan", post(start_scan))
        .route("/scan/summary", get(scan_summary))
        .route("/scan/tree", get(scan_tree))
        .route("/fs/dirs", get(fs_dirs))
        .route("/fs/suggest", get(fs_suggest))
        .route("/plan/rules", post(plan_rules))
        .route("/plan/ai", post(plan_ai))
        .route("/plans/{id}", get(get_plan).put(update_plan))
        .route("/plans/{id}/execute", post(execute_plan))
        .route("/runs", get(list_runs))
        .route("/runs/{id}/undo", post(undo_run))
        .route("/jobs/{id}", get(get_job))
        .route("/jobs/{id}/cancel", post(cancel_job))
        .route("/ai/test", post(ai_test))
        .route("/ai/ollama/models", get(ollama_models))
        .route("/ai/ollama/pull", post(ollama_pull))
        .route("/open-browser", post(open_browser))
        .route("/open-path", post(open_path))
}

// ---------- config ----------

async fn get_config(State(st): State<Shared>) -> Json<AppConfig> {
    Json(st.config.read().unwrap().redacted())
}

async fn put_config(
    State(st): State<Shared>,
    Json(incoming): Json<AppConfig>,
) -> ApiResult<AppConfig> {
    let mut cfg = st.config.write().unwrap();
    cfg.merge_from_ui(incoming);
    cfg.save()?;
    Ok(Json(cfg.redacted()))
}

// ---------- drives & benchmark ----------

async fn get_drives(State(st): State<Shared>) -> Json<Value> {
    let list = tokio::task::spawn_blocking(drives::list_drives)
        .await
        .unwrap_or_default();
    let bench = st.bench.read().unwrap().clone();
    let out: Vec<Value> = list
        .iter()
        .map(|d| {
            let mut v = serde_json::to_value(d).unwrap();
            v["used_bytes"] = json!(d.used_bytes());
            v["bench"] = serde_json::to_value(bench.get(&d.id)).unwrap();
            v["speed_score"] = json!(rules::speed_score(d, &bench));
            v
        })
        .collect();
    Json(json!({ "drives": out }))
}

#[derive(Deserialize)]
struct BenchReq {
    drive_id: String,
    size_mb: Option<u64>,
}

async fn start_bench(State(st): State<Shared>, Json(req): Json<BenchReq>) -> ApiResult<Value> {
    let list = drives::list_drives();
    let d: DriveInfo = list
        .into_iter()
        .find(|d| d.id == req.drive_id)
        .ok_or_else(|| anyhow!("Laufwerk nicht gefunden"))?;
    if d.read_only {
        return Err(anyhow!("Laufwerk ist schreibgeschützt").into());
    }
    let size = req.size_mb.unwrap_or(512);
    let need = size * 1024 * 1024 * 2;
    if d.available_bytes < need {
        return Err(anyhow!(
            "Zu wenig freier Platz für den Test ({} MB benötigt)",
            size * 2
        )
        .into());
    }
    let id = jobs::spawn(&st, "bench", move |ctx: JobCtx| async move {
        let dir = bench::bench_dir_for(&d.mount_point);
        let c2 = ctx.clone();
        let res = tokio::task::spawn_blocking(move || {
            bench::run(&dir, size, &|p| {
                let phase = if p < 0.45 {
                    "Schreiben"
                } else if p < 0.8 {
                    "Lesen"
                } else {
                    "Zufälliges Lesen 4K"
                };
                c2.progress(p, phase);
            })
        })
        .await??;
        ctx.state
            .bench
            .write()
            .unwrap()
            .insert(d.id.clone(), res.clone());
        ctx.state.save_bench();
        Ok(serde_json::to_value(res)?)
    });
    Ok(Json(json!({ "job_id": id })))
}

// ---------- scan ----------

#[derive(Deserialize)]
struct ScanReq {
    roots: Option<Vec<PathBuf>>,
}

async fn start_scan(State(st): State<Shared>, Json(req): Json<ScanReq>) -> ApiResult<Value> {
    let cfg = st.config.read().unwrap().clone();
    let roots = req
        .roots
        .filter(|r| !r.is_empty())
        .unwrap_or(cfg.scan_roots.clone());
    if roots.is_empty() {
        return Err(anyhow!("Keine Ordner zum Scannen ausgewählt").into());
    }
    let id = jobs::spawn(&st, "scan", move |ctx: JobCtx| async move {
        let c2 = ctx.clone();
        let excluded = cfg.excluded.clone();
        let idx = tokio::task::spawn_blocking(move || {
            scan::scan(&roots, &excluded, &c2.cancel, &|n| {
                c2.progress(-1.0, format!("{n} Dateien gefunden …"))
            })
        })
        .await?;
        ctx.progress(-1.0, "Suche nach Duplikaten …");
        let idx = Arc::new(idx);
        let i2 = idx.clone();
        let days = cfg.old_after_days;
        let summary = tokio::task::spawn_blocking(move || i2.summary(days)).await?;
        *ctx.state.scan.write().unwrap() = Some(idx);
        Ok(serde_json::to_value(summary)?)
    });
    Ok(Json(json!({ "job_id": id })))
}

async fn scan_summary(State(st): State<Shared>) -> ApiResult<Value> {
    let idx = st.scan.read().unwrap().clone();
    let days = st.config.read().unwrap().old_after_days;
    match idx {
        Some(i) => Ok(Json(serde_json::to_value(
            tokio::task::spawn_blocking(move || i.summary(days)).await?,
        )?)),
        None => Ok(Json(Value::Null)),
    }
}

#[derive(Deserialize)]
struct PathQ {
    path: Option<String>,
}

async fn scan_tree(State(st): State<Shared>, Query(q): Query<PathQ>) -> ApiResult<Value> {
    let idx = st
        .scan
        .read()
        .unwrap()
        .clone()
        .ok_or_else(|| anyhow!("Noch kein Scan vorhanden"))?;
    let path = match q.path.filter(|p| !p.is_empty()) {
        Some(p) => PathBuf::from(p),
        None => {
            // virtual root listing all scan roots
            let roots: Vec<Value> = idx
                .roots
                .iter()
                .map(|r| {
                    let s = idx.dirs.get(r).cloned().unwrap_or_default();
                    json!({"path": r, "size": s.size, "files": s.files, "protected": s.protected})
                })
                .collect();
            let size: u64 = roots.iter().map(|r| r["size"].as_u64().unwrap_or(0)).sum();
            return Ok(Json(
                json!({"path": "", "size": size, "files": idx.files.len(), "protected": false, "dirs": roots, "file_entries": []}),
            ));
        }
    };
    let node = idx
        .tree(&path, 300)
        .ok_or_else(|| anyhow!("Ordner ist nicht im Scan enthalten"))?;
    Ok(Json(serde_json::to_value(node)?))
}

// ---------- file system helpers for the folder picker ----------

async fn fs_dirs(Query(q): Query<PathQ>) -> ApiResult<Value> {
    let Some(p) = q.path.filter(|p| !p.is_empty()) else {
        let mut roots: Vec<Value> = vec![];
        if let Some(h) = dirs::home_dir() {
            roots.push(json!({"name": "Benutzerordner", "path": h}));
        }
        for d in drives::list_drives() {
            roots.push(json!({"name": d.mount_point.display().to_string(), "path": d.mount_point}));
        }
        return Ok(Json(json!({"path": "", "parent": null, "dirs": roots})));
    };
    let path = PathBuf::from(&p);
    let mut dirs: Vec<Value> = std::fs::read_dir(&path)
        .with_context(|| format!("Kann {} nicht öffnen", path.display()))?
        .flatten()
        .filter(|e| e.file_type().map(|t| t.is_dir()).unwrap_or(false))
        .filter(|e| !e.file_name().to_string_lossy().starts_with('.'))
        .map(|e| json!({"name": e.file_name().to_string_lossy(), "path": e.path(), "protected": drivesage_core::protect::is_protected(&e.path())}))
        .collect();
    dirs.sort_by_key(|v| v["name"].as_str().unwrap_or("").to_lowercase());
    Ok(Json(
        json!({"path": path, "parent": path.parent(), "dirs": dirs}),
    ))
}

async fn fs_suggest() -> Json<Value> {
    Json(json!({ "roots": default_scan_roots() }))
}

// ---------- plans ----------

/// Use the current scan if it covers `target`, otherwise scan `target` now.
async fn index_for(st: &Shared, target: &Path) -> anyhow::Result<Arc<ScanIndex>> {
    if let Some(idx) = st.scan.read().unwrap().clone() {
        if idx.roots.iter().any(|r| target.starts_with(r)) {
            return Ok(idx);
        }
    }
    let t = target.to_path_buf();
    let excluded = st.config.read().unwrap().excluded.clone();
    let idx = tokio::task::spawn_blocking(move || {
        scan::scan(&[t], &excluded, &Default::default(), &|_| {})
    })
    .await?;
    Ok(Arc::new(idx))
}

#[derive(Deserialize)]
struct RulesReq {
    target: PathBuf,
    options: Option<RuleOptions>,
}

async fn plan_rules(State(st): State<Shared>, Json(req): Json<RulesReq>) -> ApiResult<Plan> {
    if !req.target.is_dir() {
        return Err(anyhow!("Ordner existiert nicht").into());
    }
    let cfg = st.config.read().unwrap().clone();
    let mut opts = req.options.unwrap_or_default();
    opts.old_after_days = cfg.old_after_days;
    opts.large_file_mb = cfg.large_file_mb;
    let idx = index_for(&st, &req.target).await?;
    let bench = st.bench.read().unwrap().clone();
    let target = req.target.clone();
    let plan = tokio::task::spawn_blocking(move || {
        let drives = drives::list_drives();
        rules::build_plan(&idx, &target, &opts, &drives, &bench)
    })
    .await?;
    st.plans.write().unwrap().insert(plan.id, plan.clone());
    Ok(Json(plan))
}

#[derive(Deserialize)]
struct AiReq {
    target: PathBuf,
}

async fn plan_ai(State(st): State<Shared>, Json(req): Json<AiReq>) -> ApiResult<Value> {
    if !req.target.is_dir() {
        return Err(anyhow!("Ordner existiert nicht").into());
    }
    let cfg = st.config.read().unwrap().ai.clone();
    let client = AiClient::new(cfg)?;
    let id = jobs::spawn(&st, "ai_plan", move |ctx: JobCtx| async move {
        ctx.progress(-1.0, "Ordner wird gelesen …");
        let idx = index_for(&ctx.state, &req.target).await?;
        ctx.progress(-1.0, format!("KI denkt nach ({}) …", client.label()));
        let plan = client.build_plan(&idx, &req.target).await?;
        ctx.state
            .plans
            .write()
            .unwrap()
            .insert(plan.id, plan.clone());
        Ok(serde_json::to_value(plan)?)
    });
    Ok(Json(json!({ "job_id": id })))
}

async fn get_plan(State(st): State<Shared>, AxPath(id): AxPath<Uuid>) -> ApiResult<Plan> {
    st.plans
        .read()
        .unwrap()
        .get(&id)
        .cloned()
        .map(Json)
        .ok_or_else(|| anyhow!("Plan nicht gefunden").into())
}

#[derive(Deserialize)]
struct ActionPatch {
    id: u32,
    enabled: Option<bool>,
    to: Option<PathBuf>,
}
#[derive(Deserialize)]
struct PlanPatch {
    actions: Vec<ActionPatch>,
}

async fn update_plan(
    State(st): State<Shared>,
    AxPath(id): AxPath<Uuid>,
    Json(p): Json<PlanPatch>,
) -> ApiResult<Plan> {
    let mut plans = st.plans.write().unwrap();
    let plan = plans
        .get_mut(&id)
        .ok_or_else(|| anyhow!("Plan nicht gefunden"))?;
    for patch in p.actions {
        if let Some(a) = plan.actions.iter_mut().find(|a| a.id == patch.id) {
            if let Some(e) = patch.enabled {
                a.enabled = e;
            }
            if let Some(to) = patch.to {
                if !to.is_absolute() {
                    return Err(anyhow!("Zielpfad muss absolut sein").into());
                }
                a.to = to;
            }
        }
    }
    Ok(Json(plan.clone()))
}

#[derive(Deserialize)]
struct ExecReq {
    dry_run: bool,
}

async fn execute_plan(
    State(st): State<Shared>,
    AxPath(id): AxPath<Uuid>,
    Json(req): Json<ExecReq>,
) -> ApiResult<plan::RunReport> {
    let plan = st
        .plans
        .read()
        .unwrap()
        .get(&id)
        .cloned()
        .ok_or_else(|| anyhow!("Plan nicht gefunden"))?;
    let dd = st.data_dir.clone();
    let report =
        tokio::task::spawn_blocking(move || plan::execute(&plan, req.dry_run, &dd)).await??;
    if !req.dry_run && report.moved > 0 {
        *st.scan.write().unwrap() = None; // index is stale now
    }
    Ok(Json(report))
}

async fn list_runs(State(st): State<Shared>) -> Json<Value> {
    Json(json!({ "runs": plan::list_runs(&st.data_dir) }))
}

async fn undo_run(
    State(st): State<Shared>,
    AxPath(id): AxPath<Uuid>,
) -> ApiResult<plan::RunReport> {
    let dd = st.data_dir.clone();
    let r = tokio::task::spawn_blocking(move || plan::undo(id, &dd)).await??;
    *st.scan.write().unwrap() = None;
    Ok(Json(r))
}

// ---------- jobs ----------

async fn get_job(State(st): State<Shared>, AxPath(id): AxPath<Uuid>) -> ApiResult<jobs::Job> {
    st.jobs
        .lock()
        .unwrap()
        .get(&id)
        .cloned()
        .map(Json)
        .ok_or_else(|| anyhow!("Job nicht gefunden").into())
}

async fn cancel_job(State(st): State<Shared>, AxPath(id): AxPath<Uuid>) -> ApiResult<Value> {
    if let Some(j) = st.jobs.lock().unwrap().get(&id) {
        j.cancel.store(true, std::sync::atomic::Ordering::Relaxed);
    }
    Ok(Json(json!({"ok": true})))
}

// ---------- AI ----------

#[derive(Deserialize)]
struct AiTestReq {
    ai: Option<AiConfig>,
}

async fn ai_test(State(st): State<Shared>, Json(req): Json<AiTestReq>) -> ApiResult<Value> {
    let stored = st.config.read().unwrap().ai.clone();
    let mut cfg = req.ai.unwrap_or_else(|| stored.clone());
    if cfg
        .api_key
        .as_deref()
        .map(|k| k.starts_with("••••") || k.is_empty())
        .unwrap_or(true)
    {
        cfg.api_key = stored.api_key;
    }
    let client = AiClient::new(cfg)?;
    let reply = client.test().await?;
    Ok(Json(json!({"ok": true, "reply": reply})))
}

#[derive(Deserialize)]
struct OllamaQ {
    base_url: Option<String>,
}

async fn ollama_models(Query(q): Query<OllamaQ>) -> Json<Value> {
    let base = q
        .base_url
        .filter(|b| !b.is_empty())
        .unwrap_or_else(|| "http://127.0.0.1:11434".into());
    match ai::list_ollama_models(&base).await {
        Ok(m) => Json(json!({"running": true, "models": m})),
        Err(e) => Json(json!({"running": false, "models": [], "error": format!("{e:#}")})),
    }
}

#[derive(Deserialize)]
struct PullReq {
    model: String,
    base_url: Option<String>,
}

async fn ollama_pull(State(st): State<Shared>, Json(req): Json<PullReq>) -> ApiResult<Value> {
    let base = req
        .base_url
        .filter(|b| !b.is_empty())
        .unwrap_or_else(|| "http://127.0.0.1:11434".into());
    let id = jobs::spawn(&st, "ollama_pull", move |ctx: JobCtx| async move {
        ctx.progress(
            -1.0,
            format!("Lade {} herunter … (kann einige Minuten dauern)", req.model),
        );
        ai::ollama_pull(&base, &req.model).await?;
        Ok(json!({"model": req.model}))
    });
    Ok(Json(json!({ "job_id": id })))
}

// ---------- misc ----------

async fn open_browser(State(st): State<Shared>) -> ApiResult<Value> {
    open::that_detached(st.url())?;
    Ok(Json(json!({"ok": true})))
}

#[derive(Deserialize)]
struct OpenPathReq {
    path: PathBuf,
}

async fn open_path(Json(req): Json<OpenPathReq>) -> ApiResult<Value> {
    let dir = if req.path.is_dir() {
        req.path
    } else {
        req.path
            .parent()
            .map(Path::to_path_buf)
            .ok_or_else(|| anyhow!("Ungültiger Pfad"))?
    };
    if !dir.is_dir() {
        return Err(anyhow!("Ordner existiert nicht").into());
    }
    open::that_detached(&dir)?;
    Ok(Json(json!({"ok": true})))
}
