//! Background jobs (scan, benchmark, AI planning, model download) with progress.

use crate::Shared;
use serde::Serialize;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum JobStatus {
    Running,
    Done,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, Serialize)]
pub struct Job {
    pub id: Uuid,
    pub kind: String,
    pub status: JobStatus,
    /// 0..1, or negative when unknown.
    pub progress: f32,
    pub message: String,
    pub result: Option<serde_json::Value>,
    pub error: Option<String>,
    #[serde(skip)]
    pub cancel: Arc<AtomicBool>,
}

/// Handle passed into job bodies to report progress.
#[derive(Clone)]
pub struct JobCtx {
    pub id: Uuid,
    pub state: Shared,
    pub cancel: Arc<AtomicBool>,
}

impl JobCtx {
    pub fn progress(&self, p: f32, msg: impl Into<String>) {
        if let Some(j) = self.state.jobs.lock().unwrap().get_mut(&self.id) {
            j.progress = p;
            j.message = msg.into();
        }
    }
}

/// Spawn a job. `body` runs on the tokio runtime and returns a JSON result.
pub fn spawn<F, Fut>(state: &Shared, kind: &str, body: F) -> Uuid
where
    F: FnOnce(JobCtx) -> Fut + Send + 'static,
    Fut: std::future::Future<Output = anyhow::Result<serde_json::Value>> + Send + 'static,
{
    let id = Uuid::new_v4();
    let cancel = Arc::new(AtomicBool::new(false));
    state.jobs.lock().unwrap().insert(
        id,
        Job {
            id,
            kind: kind.into(),
            status: JobStatus::Running,
            progress: -1.0,
            message: "Startet …".into(),
            result: None,
            error: None,
            cancel: cancel.clone(),
        },
    );
    let ctx = JobCtx {
        id,
        state: state.clone(),
        cancel,
    };
    let st = state.clone();
    tokio::spawn(async move {
        let cancel = ctx.cancel.clone();
        let res = body(ctx).await;
        if let Some(j) = st.jobs.lock().unwrap().get_mut(&id) {
            match res {
                Ok(v) => {
                    j.status = if cancel.load(Ordering::Relaxed) {
                        JobStatus::Cancelled
                    } else {
                        JobStatus::Done
                    };
                    j.progress = 1.0;
                    j.result = Some(v);
                    j.message = "Fertig".into();
                }
                Err(e) => {
                    j.status = JobStatus::Failed;
                    j.error = Some(format!("{e:#}"));
                    j.message = "Fehlgeschlagen".into();
                }
            }
        }
    });
    id
}
