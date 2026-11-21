//! Local HTTP server: JSON API + embedded web UI.
//!
//! Security model: binds to 127.0.0.1 only, every API call needs a random
//! per-launch token (sent as `x-drivesage-token` header), and the Host header
//! must be localhost — this blocks other websites (CSRF / DNS rebinding).

mod api;
mod jobs;
mod web;

use anyhow::Result;
use axum::{
    body::Body,
    extract::State,
    http::{header, Request, StatusCode},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    Router,
};
use drivesage_core::{
    bench::BenchResult, config::data_dir, plan::Plan, scan::ScanIndex, AppConfig,
};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, RwLock};
use tokio::net::TcpListener;
use uuid::Uuid;

pub use jobs::{Job, JobStatus};

pub struct AppState {
    pub token: String,
    pub port: u16,
    pub data_dir: PathBuf,
    pub config: RwLock<AppConfig>,
    pub scan: RwLock<Option<Arc<ScanIndex>>>,
    pub bench: RwLock<HashMap<String, BenchResult>>,
    pub plans: RwLock<HashMap<Uuid, Plan>>,
    pub jobs: Mutex<HashMap<Uuid, Job>>,
}

impl AppState {
    pub fn url(&self) -> String {
        format!("http://127.0.0.1:{}/#token={}", self.port, self.token)
    }
    fn bench_file(&self) -> PathBuf {
        self.data_dir.join("bench.json")
    }
    pub fn save_bench(&self) {
        if let Ok(s) = serde_json::to_string_pretty(&*self.bench.read().unwrap()) {
            let _ = std::fs::create_dir_all(&self.data_dir);
            let _ = std::fs::write(self.bench_file(), s);
        }
    }
}

pub type Shared = Arc<AppState>;

pub struct ServerOptions {
    /// 0 = pick a free port.
    pub port: u16,
}

pub struct RunningServer {
    pub state: Shared,
    pub addr: SocketAddr,
    pub handle: tokio::task::JoinHandle<()>,
}

impl RunningServer {
    pub fn url(&self) -> String {
        self.state.url()
    }
}

async fn guard(State(st): State<Shared>, req: Request<Body>, next: Next) -> Response {
    let host = req
        .headers()
        .get(header::HOST)
        .and_then(|h| h.to_str().ok())
        .unwrap_or("");
    let host_ok =
        host == format!("127.0.0.1:{}", st.port) || host == format!("localhost:{}", st.port);
    if !host_ok {
        return (StatusCode::FORBIDDEN, "bad host").into_response();
    }
    let path = req.uri().path();
    if path.starts_with("/api/") && path != "/api/health" {
        let tok = req
            .headers()
            .get("x-drivesage-token")
            .and_then(|h| h.to_str().ok())
            .unwrap_or("");
        if tok != st.token {
            return (StatusCode::UNAUTHORIZED, "missing or wrong token").into_response();
        }
    }
    next.run(req).await
}

pub async fn start(opts: ServerOptions) -> Result<RunningServer> {
    let listener = TcpListener::bind(("127.0.0.1", opts.port)).await?;
    let addr = listener.local_addr()?;
    let dd = data_dir();
    let bench = std::fs::read_to_string(dd.join("bench.json"))
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default();
    let state: Shared = Arc::new(AppState {
        token: Uuid::new_v4().simple().to_string(),
        port: addr.port(),
        data_dir: dd,
        config: RwLock::new(AppConfig::load()),
        scan: RwLock::new(None),
        bench: RwLock::new(bench),
        plans: RwLock::new(HashMap::new()),
        jobs: Mutex::new(HashMap::new()),
    });

    let app = Router::new()
        .nest("/api", api::routes())
        .fallback(web::static_handler)
        .layer(middleware::from_fn_with_state(state.clone(), guard))
        .with_state(state.clone());

    let handle = tokio::spawn(async move {
        if let Err(e) = axum::serve(listener, app).await {
            tracing::error!("server stopped: {e}");
        }
    });
    Ok(RunningServer {
        state,
        addr,
        handle,
    })
}
