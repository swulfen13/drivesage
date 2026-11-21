use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use sysinfo::{DiskKind, Disks};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DriveKind {
    Ssd,
    Hdd,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DriveInfo {
    /// Stable id for the UI (the mount point as string).
    pub id: String,
    pub name: String,
    pub mount_point: PathBuf,
    pub file_system: String,
    pub kind: DriveKind,
    pub removable: bool,
    pub read_only: bool,
    pub total_bytes: u64,
    pub available_bytes: u64,
}

impl DriveInfo {
    pub fn used_bytes(&self) -> u64 {
        self.total_bytes.saturating_sub(self.available_bytes)
    }
}

/// File systems that are virtual / not interesting for the user.
fn is_virtual_fs(fs: &str) -> bool {
    matches!(
        fs,
        "tmpfs"
            | "devtmpfs"
            | "overlay"
            | "squashfs"
            | "proc"
            | "sysfs"
            | "cgroup"
            | "cgroup2"
            | "fuse.snapfuse"
            | "autofs"
            | "devfs"
            | "ramfs"
            | "efivarfs"
            | "9p"
            | "virtiofs"
    )
}

/// List physical drives / partitions that are mounted.
pub fn list_drives() -> Vec<DriveInfo> {
    let Ok(disks) = Disks::new_with_refreshed_list() else {
        return vec![];
    };
    let mut out: Vec<DriveInfo> = (&disks)
        .into_iter()
        .filter_map(|d| {
            let fs = d.file_system().to_string_lossy().to_string();
            if is_virtual_fs(&fs) || d.total_space() == 0 {
                return None;
            }
            let mount = d.mount_point().to_path_buf();
            let mount_s = mount.to_string_lossy().to_string();
            if mount_s.starts_with("/snap") || mount_s.starts_with("/boot") {
                return None;
            }
            let kind = match d.kind() {
                DiskKind::SSD => DriveKind::Ssd,
                DiskKind::HDD => DriveKind::Hdd,
                _ => DriveKind::Unknown,
            };
            Some(DriveInfo {
                id: mount_s,
                name: d.name().to_string_lossy().to_string(),
                mount_point: mount,
                file_system: fs,
                kind,
                removable: d.is_removable(),
                read_only: d.is_read_only(),
                total_bytes: d.total_space(),
                available_bytes: d.available_space(),
            })
        })
        .collect();
    out.sort_by(|a, b| a.mount_point.cmp(&b.mount_point));
    out.dedup_by(|a, b| a.mount_point == b.mount_point);
    out
}

/// Find the drive a path lives on (longest matching mount point).
pub fn drive_for<'a>(path: &Path, drives: &'a [DriveInfo]) -> Option<&'a DriveInfo> {
    drives
        .iter()
        .filter(|d| path.starts_with(&d.mount_point))
        .max_by_key(|d| d.mount_point.as_os_str().len())
}
