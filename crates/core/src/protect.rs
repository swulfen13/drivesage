//! Safety rules: which paths DriveSage must never touch.

use std::path::Path;

/// Directory names that are always protected (case-insensitive), wherever they appear.
const PROTECTED_NAMES: &[&str] = &[
    "windows",
    "program files",
    "program files (x86)",
    "programdata",
    "appdata",
    "$recycle.bin",
    "system volume information",
    "recovery",
    "boot",
    ".git",
    ".svn",
    "node_modules",
    ".venv",
    "venv",
    "target",
    "library", // macOS ~/Library
    "steamapps",
    ".ssh",
    ".gnupg",
];

/// Unix system roots that are protected.
const PROTECTED_ROOTS: &[&str] = &[
    "/bin",
    "/boot",
    "/dev",
    "/etc",
    "/lib",
    "/lib64",
    "/proc",
    "/sbin",
    "/sys",
    "/usr",
    "/var",
    "/System",
    "/Applications",
];

/// Files that must never be moved.
const PROTECTED_FILES: &[&str] = &[
    "pagefile.sys",
    "hiberfil.sys",
    "swapfile.sys",
    "desktop.ini",
    "thumbs.db",
];

/// Returns true if `path` (or any of its ancestors) is protected.
pub fn is_protected(path: &Path) -> bool {
    let s = path.to_string_lossy();
    for root in PROTECTED_ROOTS {
        if s == *root || s.starts_with(&format!("{root}/")) {
            return true;
        }
    }
    for comp in path.components() {
        let c = comp.as_os_str().to_string_lossy().to_lowercase();
        // hidden folders/files (".config", ".cache", ".env" …) are left alone
        let hidden = c.starts_with('.') && c != "." && c != "..";
        if hidden || PROTECTED_NAMES.contains(&c.as_str()) {
            return true;
        }
    }
    if let Some(name) = path.file_name() {
        let n = name.to_string_lossy().to_lowercase();
        if PROTECTED_FILES.contains(&n.as_str()) {
            return true;
        }
    }
    false
}

/// A directory that looks like a code project (contains .git, Cargo.toml, package.json …).
/// Files inside such a project are never reorganised, because moving them breaks builds.
pub fn is_project_dir(dir: &Path) -> bool {
    const MARKERS: &[&str] = &[
        ".git",
        "Cargo.toml",
        "package.json",
        "pyproject.toml",
        ".sln",
        "go.mod",
        "pom.xml",
    ];
    MARKERS.iter().any(|m| dir.join(m).exists())
}

/// True if any ancestor of `path` (up to and excluding `stop_at`) is a project directory.
pub fn inside_project(path: &Path, stop_at: &Path) -> bool {
    let mut cur = path.parent();
    while let Some(dir) = cur {
        if dir == stop_at || !dir.starts_with(stop_at) {
            break;
        }
        if is_project_dir(dir) {
            return true;
        }
        cur = dir.parent();
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn protects_system_paths() {
        assert!(is_protected(Path::new(
            "C:\\Windows\\System32\\x.dll".replace('\\', "/").as_str()
        )));
        assert!(is_protected(Path::new("/home/u/AppData/Roaming/x")));
        assert!(is_protected(Path::new("/home/u/code/.git/config")));
        assert!(is_protected(Path::new("/etc/passwd")));
        assert!(!is_protected(Path::new("/home/u/Downloads/rechnung.pdf")));
    }
}
