use serde::{Deserialize, Serialize};
use std::path::Path;

/// Coarse file categories used for statistics and the "sort by type" rule.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Category {
    Documents,
    Images,
    Videos,
    Audio,
    Archives,
    Installers,
    Code,
    Other,
}

impl Category {
    pub const ALL: [Category; 8] = [
        Category::Documents,
        Category::Images,
        Category::Videos,
        Category::Audio,
        Category::Archives,
        Category::Installers,
        Category::Code,
        Category::Other,
    ];

    /// Folder name used when sorting files into category folders (German UI default).
    pub fn folder_name(self) -> &'static str {
        match self {
            Category::Documents => "Dokumente",
            Category::Images => "Bilder",
            Category::Videos => "Videos",
            Category::Audio => "Musik & Audio",
            Category::Archives => "Archive",
            Category::Installers => "Installer",
            Category::Code => "Code",
            Category::Other => "Sonstiges",
        }
    }

    pub fn of(path: &Path) -> Category {
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.to_ascii_lowercase())
            .unwrap_or_default();
        match ext.as_str() {
            "pdf" | "doc" | "docx" | "odt" | "rtf" | "txt" | "md" | "xls" | "xlsx" | "ods"
            | "csv" | "ppt" | "pptx" | "odp" | "pages" | "numbers" | "key" | "epub" => {
                Category::Documents
            }
            "jpg" | "jpeg" | "png" | "gif" | "bmp" | "webp" | "heic" | "heif" | "tif" | "tiff"
            | "svg" | "raw" | "cr2" | "nef" | "arw" | "psd" | "ico" => Category::Images,
            "mp4" | "mkv" | "mov" | "avi" | "wmv" | "webm" | "m4v" | "flv" | "mpg" | "mpeg" => {
                Category::Videos
            }
            "mp3" | "wav" | "flac" | "aac" | "ogg" | "m4a" | "wma" | "opus" => Category::Audio,
            "zip" | "rar" | "7z" | "tar" | "gz" | "bz2" | "xz" | "zst" | "iso" => {
                Category::Archives
            }
            "exe" | "msi" | "msix" | "appx" | "dmg" | "pkg" | "deb" | "rpm" | "appimage" => {
                Category::Installers
            }
            "rs" | "py" | "js" | "ts" | "tsx" | "jsx" | "java" | "c" | "cpp" | "h" | "hpp"
            | "cs" | "go" | "rb" | "php" | "html" | "css" | "json" | "yaml" | "yml" | "toml"
            | "sh" | "ps1" | "bat" | "ipynb" | "sql" => Category::Code,
            _ => Category::Other,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn detects_categories() {
        assert_eq!(
            Category::of(Path::new("a/Rechnung.PDF")),
            Category::Documents
        );
        assert_eq!(Category::of(Path::new("setup.exe")), Category::Installers);
        assert_eq!(Category::of(Path::new("noext")), Category::Other);
    }
}
