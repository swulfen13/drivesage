//! Fast parallel scan of one or more folders.

use crate::categories::Category;
use crate::protect::is_protected;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileEntry {
    pub path: PathBuf,
    pub size: u64,
    /// Unix seconds.
    pub modified: Option<i64>,
    /// Unix seconds. Note: Windows often doesn't update access times.
    pub accessed: Option<i64>,
    pub category: Category,
    pub protected: bool,
}

impl FileEntry {
    /// Most recent of modified / accessed.
    pub fn last_used(&self) -> Option<i64> {
        match (self.modified, self.accessed) {
            (Some(a), Some(b)) => Some(a.max(b)),
            (a, b) => a.or(b),
        }
    }
    pub fn age_days(&self, now: i64) -> Option<i64> {
        self.last_used().map(|t| (now - t) / 86_400)
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DirStat {
    pub size: u64,
    pub files: u64,
    pub protected: bool,
}

/// Full in-memory index of a scan.
#[derive(Debug, Default)]
pub struct ScanIndex {
    pub roots: Vec<PathBuf>,
    pub files: Vec<FileEntry>,
    pub dirs: HashMap<PathBuf, DirStat>,
    pub subdirs: HashMap<PathBuf, Vec<PathBuf>>,
    pub files_by_dir: HashMap<PathBuf, Vec<usize>>,
    pub errors: u64,
    pub duration_ms: u128,
    pub cancelled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CategoryStat {
    pub category: Category,
    pub bytes: u64,
    pub files: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DirEntryOut {
    pub path: PathBuf,
    pub size: u64,
    pub files: u64,
    pub protected: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DuplicateGroup {
    pub size: u64,
    pub paths: Vec<PathBuf>,
    /// Bytes that could be freed by keeping only one copy.
    pub wasted: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanSummary {
    pub roots: Vec<PathBuf>,
    pub total_bytes: u64,
    pub total_files: u64,
    pub total_dirs: u64,
    pub by_category: Vec<CategoryStat>,
    pub largest_files: Vec<FileEntry>,
    pub largest_dirs: Vec<DirEntryOut>,
    pub old_bytes: u64,
    pub old_files: u64,
    pub duplicates: Vec<DuplicateGroup>,
    pub duplicate_wasted_bytes: u64,
    pub errors: u64,
    pub duration_ms: u128,
    pub cancelled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TreeNode {
    pub path: PathBuf,
    pub size: u64,
    pub files: u64,
    pub protected: bool,
    pub dirs: Vec<DirEntryOut>,
    pub file_entries: Vec<FileEntry>,
}

pub fn unix_now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

fn to_unix(t: std::io::Result<SystemTime>) -> Option<i64> {
    t.ok()?
        .duration_since(UNIX_EPOCH)
        .ok()
        .map(|d| d.as_secs() as i64)
}

/// Scan the given roots. `progress(files_seen)` is called periodically.
pub fn scan(
    roots: &[PathBuf],
    excluded: &[PathBuf],
    cancel: &AtomicBool,
    progress: &dyn Fn(u64),
) -> ScanIndex {
    let started = Instant::now();
    let mut idx = ScanIndex {
        roots: roots.to_vec(),
        ..Default::default()
    };

    for root in roots {
        if !root.exists() {
            idx.errors += 1;
            continue;
        }
        idx.dirs.entry(root.clone()).or_insert_with(|| DirStat {
            protected: is_protected(root),
            ..Default::default()
        });
        let excluded_owned: Vec<PathBuf> = excluded.to_vec();
        let walker = jwalk::WalkDir::new(root)
            .skip_hidden(false)
            .follow_links(false)
            .process_read_dir(move |_, _, _, children| {
                children.retain(|c| match c {
                    Ok(e) => !excluded_owned.iter().any(|x| e.path().starts_with(x)),
                    Err(_) => true,
                });
            });

        for entry in walker {
            if cancel.load(Ordering::Relaxed) {
                idx.cancelled = true;
                break;
            }
            let entry = match entry {
                Ok(e) => e,
                Err(_) => {
                    idx.errors += 1;
                    continue;
                }
            };
            let path = entry.path();
            let ft = entry.file_type();
            if ft.is_dir() {
                if path != *root {
                    if let Some(parent) = path.parent() {
                        idx.subdirs
                            .entry(parent.to_path_buf())
                            .or_default()
                            .push(path.clone());
                    }
                    let prot = is_protected(&path);
                    idx.dirs.entry(path).or_insert_with(|| DirStat {
                        protected: prot,
                        ..Default::default()
                    });
                }
                continue;
            }
            if !ft.is_file() {
                continue;
            }
            let md = match entry.metadata() {
                Ok(m) => m,
                Err(_) => {
                    idx.errors += 1;
                    continue;
                }
            };
            let size = md.len();
            let fe = FileEntry {
                category: Category::of(&path),
                protected: is_protected(&path),
                modified: to_unix(md.modified()),
                accessed: to_unix(md.accessed()),
                size,
                path: path.clone(),
            };
            let i = idx.files.len();
            idx.files.push(fe);
            if let Some(parent) = path.parent() {
                idx.files_by_dir
                    .entry(parent.to_path_buf())
                    .or_default()
                    .push(i);
                // add size to every ancestor up to (and including) the root
                let mut cur = Some(parent);
                while let Some(d) = cur {
                    let st = idx.dirs.entry(d.to_path_buf()).or_default();
                    st.size += size;
                    st.files += 1;
                    if d == root.as_path() {
                        break;
                    }
                    cur = d.parent();
                }
            }
            if idx.files.len() % 2000 == 0 {
                progress(idx.files.len() as u64);
            }
        }
    }
    progress(idx.files.len() as u64);
    idx.duration_ms = started.elapsed().as_millis();
    idx
}

impl ScanIndex {
    pub fn summary(&self, old_after_days: u32) -> ScanSummary {
        let now = unix_now();
        let mut cats: HashMap<Category, (u64, u64)> = HashMap::new();
        let mut total = 0u64;
        let mut old_bytes = 0u64;
        let mut old_files = 0u64;
        for f in &self.files {
            total += f.size;
            let e = cats.entry(f.category).or_default();
            e.0 += f.size;
            e.1 += 1;
            if f.age_days(now)
                .map(|d| d >= old_after_days as i64)
                .unwrap_or(false)
            {
                old_bytes += f.size;
                old_files += 1;
            }
        }
        let mut by_category: Vec<CategoryStat> = cats
            .into_iter()
            .map(|(category, (bytes, files))| CategoryStat {
                category,
                bytes,
                files,
            })
            .collect();
        by_category.sort_by(|a, b| b.bytes.cmp(&a.bytes));

        let mut largest: Vec<&FileEntry> = self.files.iter().collect();
        largest.sort_by(|a, b| b.size.cmp(&a.size));
        let largest_files = largest.into_iter().take(50).cloned().collect();

        // Largest directories: only "leaf-ish" levels below the roots (depth 1..=3) to stay meaningful.
        let mut dirs: Vec<DirEntryOut> = self
            .dirs
            .iter()
            .filter(|(p, _)| {
                self.roots.iter().any(|r| {
                    p.starts_with(r) && {
                        let depth = p
                            .components()
                            .count()
                            .saturating_sub(r.components().count());
                        (1..=3).contains(&depth)
                    }
                })
            })
            .map(|(p, s)| DirEntryOut {
                path: p.clone(),
                size: s.size,
                files: s.files,
                protected: s.protected,
            })
            .collect();
        dirs.sort_by(|a, b| b.size.cmp(&a.size));
        dirs.truncate(30);

        let duplicates = self.find_duplicates(1024 * 1024, 3000);
        let duplicate_wasted_bytes = duplicates.iter().map(|d| d.wasted).sum();

        ScanSummary {
            roots: self.roots.clone(),
            total_bytes: total,
            total_files: self.files.len() as u64,
            total_dirs: self.dirs.len() as u64,
            by_category,
            largest_files,
            largest_dirs: dirs,
            old_bytes,
            old_files,
            duplicates,
            duplicate_wasted_bytes,
            errors: self.errors,
            duration_ms: self.duration_ms,
            cancelled: self.cancelled,
        }
    }

    /// Children of a directory, largest first.
    pub fn tree(&self, path: &Path, limit: usize) -> Option<TreeNode> {
        let st = self.dirs.get(path)?;
        let mut dirs: Vec<DirEntryOut> = self
            .subdirs
            .get(path)
            .map(|v| {
                v.iter()
                    .map(|p| {
                        let s = self.dirs.get(p).cloned().unwrap_or_default();
                        DirEntryOut {
                            path: p.clone(),
                            size: s.size,
                            files: s.files,
                            protected: s.protected,
                        }
                    })
                    .collect()
            })
            .unwrap_or_default();
        dirs.sort_by(|a, b| b.size.cmp(&a.size));
        dirs.truncate(limit);
        let mut files: Vec<FileEntry> = self
            .files_by_dir
            .get(path)
            .map(|v| v.iter().map(|&i| self.files[i].clone()).collect())
            .unwrap_or_default();
        files.sort_by(|a, b| b.size.cmp(&a.size));
        files.truncate(limit);
        Some(TreeNode {
            path: path.to_path_buf(),
            size: st.size,
            files: st.files,
            protected: st.protected,
            dirs,
            file_entries: files,
        })
    }

    /// Likely duplicates: same size + same fingerprint (first, middle and last 64 KiB).
    pub fn find_duplicates(&self, min_size: u64, max_candidates: usize) -> Vec<DuplicateGroup> {
        let mut by_size: HashMap<u64, Vec<usize>> = HashMap::new();
        for (i, f) in self.files.iter().enumerate() {
            if f.size >= min_size && !f.protected {
                by_size.entry(f.size).or_default().push(i);
            }
        }
        let mut groups_out = Vec::new();
        let mut checked = 0usize;
        let mut sizes: Vec<_> = by_size.into_iter().filter(|(_, v)| v.len() > 1).collect();
        sizes.sort_by(|a, b| b.0.cmp(&a.0)); // biggest savings first
        for (size, idxs) in sizes {
            if checked >= max_candidates {
                break;
            }
            let mut by_fp: HashMap<u64, Vec<PathBuf>> = HashMap::new();
            for i in idxs {
                checked += 1;
                let p = &self.files[i].path;
                if let Some(fp) = fingerprint(p, size) {
                    by_fp.entry(fp).or_default().push(p.clone());
                }
            }
            for (_, paths) in by_fp {
                if paths.len() > 1 {
                    let wasted = size * (paths.len() as u64 - 1);
                    groups_out.push(DuplicateGroup {
                        size,
                        paths,
                        wasted,
                    });
                }
            }
        }
        groups_out.sort_by(|a, b| b.wasted.cmp(&a.wasted));
        groups_out.truncate(100);
        groups_out
    }
}

/// FNV-1a over three 64 KiB samples of the file.
fn fingerprint(path: &Path, size: u64) -> Option<u64> {
    const CHUNK: u64 = 64 * 1024;
    let mut f = std::fs::File::open(path).ok()?;
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    let mut buf = vec![0u8; CHUNK as usize];
    let offsets = [0, size / 2, size.saturating_sub(CHUNK)];
    for off in offsets {
        f.seek(SeekFrom::Start(off)).ok()?;
        let n = f.read(&mut buf).ok()?;
        for b in &buf[..n] {
            h ^= *b as u64;
            h = h.wrapping_mul(0x0100_0000_01b3);
        }
    }
    Some(h)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn scans_and_aggregates() {
        let d = tempfile::Builder::new().prefix("dstest").tempdir().unwrap();
        let root = d.path().to_path_buf();
        fs::create_dir_all(root.join("a/b")).unwrap();
        fs::write(root.join("a/b/x.pdf"), vec![1u8; 2_000_000]).unwrap();
        fs::write(root.join("a/y.pdf"), vec![1u8; 2_000_000]).unwrap();
        fs::write(root.join("z.jpg"), b"img").unwrap();
        let idx = scan(&[root.clone()], &[], &AtomicBool::new(false), &|_| {});
        assert_eq!(idx.files.len(), 3);
        assert_eq!(idx.dirs[&root].size, 4_000_003);
        assert_eq!(idx.dirs[&root.join("a")].size, 4_000_000);
        let s = idx.summary(180);
        assert_eq!(s.duplicates.len(), 1);
        assert_eq!(s.duplicate_wasted_bytes, 2_000_000);
        let t = idx.tree(&root, 10).unwrap();
        assert_eq!(t.dirs.len(), 1);
        assert_eq!(t.file_entries.len(), 1);
    }
}
