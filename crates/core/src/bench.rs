//! Drive speed test: sequential read/write and random 4K reads.
//!
//! To get honest numbers the OS page cache is bypassed where possible
//! (`FILE_FLAG_NO_BUFFERING` on Windows, `O_DIRECT` on Linux). If the drive
//! or file system refuses unbuffered I/O we fall back to buffered I/O and
//! report `cache_bypassed: false`, so the UI can show a hint.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::alloc::{alloc_zeroed, dealloc, Layout};
use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

const ALIGN: usize = 4096;
const BLOCK: usize = 4 * 1024 * 1024; // 4 MiB sequential blocks
const RAND_BLOCK: usize = 4096;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchResult {
    pub target_dir: PathBuf,
    pub size_mb: u64,
    pub seq_write_mbps: f64,
    pub seq_read_mbps: f64,
    pub rand4k_read_iops: f64,
    pub rand4k_read_mbps: f64,
    pub cache_bypassed: bool,
    pub duration_ms: u128,
}

/// Page-aligned heap buffer (required for unbuffered I/O).
struct AlignedBuf {
    ptr: *mut u8,
    layout: Layout,
}

impl AlignedBuf {
    fn new(len: usize) -> Self {
        let layout = Layout::from_size_align(len, ALIGN).expect("layout");
        // SAFETY: layout has non-zero size.
        let ptr = unsafe { alloc_zeroed(layout) };
        assert!(!ptr.is_null(), "allocation failed");
        Self { ptr, layout }
    }
    fn as_mut(&mut self) -> &mut [u8] {
        // SAFETY: ptr is valid for layout.size() bytes and uniquely owned.
        unsafe { std::slice::from_raw_parts_mut(self.ptr, self.layout.size()) }
    }
    fn as_ref(&self) -> &[u8] {
        // SAFETY: see above.
        unsafe { std::slice::from_raw_parts(self.ptr, self.layout.size()) }
    }
}
impl Drop for AlignedBuf {
    fn drop(&mut self) {
        // SAFETY: allocated with the same layout.
        unsafe { dealloc(self.ptr, self.layout) }
    }
}
unsafe impl Send for AlignedBuf {}

fn open_unbuffered(path: &Path, write: bool) -> std::io::Result<File> {
    let mut o = OpenOptions::new();
    if write {
        o.write(true).create(true).truncate(true);
    } else {
        o.read(true);
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        const FILE_FLAG_NO_BUFFERING: u32 = 0x2000_0000;
        const FILE_FLAG_WRITE_THROUGH: u32 = 0x8000_0000;
        o.custom_flags(FILE_FLAG_NO_BUFFERING | if write { FILE_FLAG_WRITE_THROUGH } else { 0 });
    }
    #[cfg(target_os = "linux")]
    {
        use std::os::unix::fs::OpenOptionsExt;
        o.custom_flags(libc::O_DIRECT);
    }
    o.open(path)
}

fn open_buffered(path: &Path, write: bool) -> std::io::Result<File> {
    let mut o = OpenOptions::new();
    if write {
        o.write(true).create(true).truncate(true);
    } else {
        o.read(true);
    }
    o.open(path)
}

/// Tiny xorshift PRNG — good enough for random offsets, no extra dependency.
struct XorShift(u64);
impl XorShift {
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }
}

/// Pick a writable directory on the drive mounted at `mount`.
/// For the system drive we use the temp dir, for others a hidden folder in the root.
pub fn bench_dir_for(mount: &Path) -> PathBuf {
    let tmp = std::env::temp_dir();
    if tmp.starts_with(mount) {
        return tmp.join("drivesage-bench");
    }
    if let Some(home) = dirs::home_dir() {
        if home.starts_with(mount) && mount.as_os_str().len() > 1 {
            return home.join(".drivesage-bench");
        }
    }
    mount.join(".drivesage-bench")
}

/// Run the benchmark. `progress` receives values in 0.0..=1.0.
pub fn run(target_dir: &Path, size_mb: u64, progress: &dyn Fn(f32)) -> Result<BenchResult> {
    let size_mb = size_mb.clamp(16, 8192);
    std::fs::create_dir_all(target_dir)
        .with_context(|| format!("cannot create {}", target_dir.display()))?;
    let file = target_dir.join(format!("drivesage-{}.tmp", uuid::Uuid::new_v4()));
    let started = Instant::now();
    let res = run_inner(&file, size_mb, progress);
    let _ = std::fs::remove_file(&file);
    let _ = std::fs::remove_dir(target_dir); // only succeeds if empty — never deletes user data
    let mut r = res?;
    r.target_dir = target_dir.to_path_buf();
    r.duration_ms = started.elapsed().as_millis();
    Ok(r)
}

fn run_inner(file: &Path, size_mb: u64, progress: &dyn Fn(f32)) -> Result<BenchResult> {
    let total = size_mb as usize * 1024 * 1024;
    let blocks = total / BLOCK;
    let mut buf = AlignedBuf::new(BLOCK);
    // Fill with pseudo-random data so compressing controllers can't cheat.
    let mut rng = XorShift(0x9E37_79B9_7F4A_7C15);
    for chunk in buf.as_mut().chunks_mut(8) {
        let v = rng.next().to_le_bytes();
        chunk.copy_from_slice(&v[..chunk.len()]);
    }

    // --- sequential write ---
    let (mut f, mut bypass) = match open_unbuffered(file, true) {
        Ok(f) => (f, true),
        Err(_) => (open_buffered(file, true)?, false),
    };
    let t = Instant::now();
    for i in 0..blocks {
        if let Err(e) = f.write_all(buf.as_ref()) {
            if bypass && i == 0 {
                // File system rejected direct I/O at write time → retry buffered.
                drop(f);
                bypass = false;
                f = open_buffered(file, true)?;
                f.write_all(buf.as_ref())?;
            } else {
                return Err(e.into());
            }
        }
        progress(0.45 * (i + 1) as f32 / blocks as f32);
    }
    f.sync_all()?;
    let seq_write = mbps(total, t.elapsed());
    drop(f);

    // --- sequential read ---
    let mut f = if bypass {
        open_unbuffered(file, false)?
    } else {
        open_buffered(file, false)?
    };
    let t = Instant::now();
    for i in 0..blocks {
        f.read_exact(buf.as_mut())?;
        progress(0.45 + 0.35 * (i + 1) as f32 / blocks as f32);
    }
    let seq_read = mbps(total, t.elapsed());

    // --- random 4K read (time-boxed) ---
    let mut small = AlignedBuf::new(RAND_BLOCK);
    let slots = (total / RAND_BLOCK) as u64;
    let budget = Duration::from_secs(4);
    let t = Instant::now();
    let mut ops: u64 = 0;
    while t.elapsed() < budget {
        let off = (rng.next() % slots) * RAND_BLOCK as u64;
        f.seek(SeekFrom::Start(off))?;
        f.read_exact(small.as_mut())?;
        ops += 1;
        if ops % 256 == 0 {
            progress(0.8 + 0.2 * (t.elapsed().as_secs_f32() / budget.as_secs_f32()).min(1.0));
        }
    }
    let secs = t.elapsed().as_secs_f64();
    let iops = ops as f64 / secs;
    progress(1.0);

    Ok(BenchResult {
        target_dir: PathBuf::new(),
        size_mb,
        seq_write_mbps: seq_write,
        seq_read_mbps: seq_read,
        rand4k_read_iops: iops,
        rand4k_read_mbps: iops * RAND_BLOCK as f64 / 1_048_576.0,
        cache_bypassed: bypass,
        duration_ms: 0,
    })
}

fn mbps(bytes: usize, d: Duration) -> f64 {
    let s = d.as_secs_f64().max(1e-6);
    bytes as f64 / 1_048_576.0 / s
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn small_benchmark_runs() {
        let dir = tempfile::Builder::new().prefix("dstest").tempdir_in(env!("CARGO_MANIFEST_DIR")).unwrap();
        let r = run(&dir.path().join("b"), 16, &|_| {}).unwrap();
        assert!(r.seq_write_mbps > 0.0 && r.seq_read_mbps > 0.0 && r.rand4k_read_iops > 0.0);
    }
}
