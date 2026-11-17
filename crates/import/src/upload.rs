//! Uploaded files: stored as UPLOAD_DIR/<32 hex>/<sanitized name>, removed after a successful
//! import or after 24 hours.

use std::path::{Path, PathBuf};
use std::sync::LazyLock;
use std::time::{Duration, SystemTime};

use regex::Regex;

use crate::problem::{Problem, Result};

pub const MAX_UPLOAD_BYTES: u64 = 1024 * 1024 * 1024;
pub const UPLOAD_MAX_AGE: Duration = Duration::from_secs(24 * 3600);
pub const TOO_BIG: &str = "That file is over the 1 GB upload limit. Split it or import a smaller file.";

static ID_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^[0-9a-f]{32}$").expect("regex"));
static BAD_CHARS_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"[^\w.\- ]+").expect("regex"));
static SPACES_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\s+").expect("regex"));

/// Delete upload folders not modified for `max_age`. Returns how many were removed.
pub fn cleanup_old_uploads(root: &Path, max_age: Duration) -> usize {
    let Ok(entries) = std::fs::read_dir(root) else { return 0 };
    let cutoff = SystemTime::now().checked_sub(max_age).unwrap_or(SystemTime::UNIX_EPOCH);
    let mut removed = 0;
    for entry in entries.flatten() {
        let Ok(meta) = entry.metadata() else { continue };
        let Ok(modified) = meta.modified() else { continue };
        if modified < cutoff {
            let path = entry.path();
            let ok = if meta.is_dir() { std::fs::remove_dir_all(&path).is_ok() } else { std::fs::remove_file(&path).is_ok() };
            if ok {
                removed += 1;
            }
        }
    }
    removed
}

/// The last path segment, with anything but word characters, dots, dashes and spaces replaced.
pub fn sanitize_filename(name: Option<&str>) -> String {
    let raw = name.unwrap_or("").replace('\\', "/");
    let base = raw.rsplit('/').next().unwrap_or("");
    let base = BAD_CHARS_RE.replace_all(base, "_");
    let base = base.trim_matches(|c| c == ' ' || c == '.');
    let mut base = SPACES_RE.replace_all(base, " ").into_owned();
    if base.chars().count() > 120 {
        base = match base.rsplit_once('.') {
            Some((stem, ext)) if ext.chars().count() <= 15 => {
                format!("{}.{}", stem.chars().take(100).collect::<String>(), ext)
            }
            _ => base.chars().take(120).collect(),
        };
    }
    if base.is_empty() {
        "upload".into()
    } else {
        base
    }
}

pub fn valid_id(id: &str) -> bool {
    ID_RE.is_match(id)
}

/// The uploaded file for an id.
pub fn upload_file(root: &Path, id: &str) -> Result<PathBuf> {
    if !valid_id(id) {
        return Err(Problem::status(404, "That upload was not found. Upload the file again."));
    }
    let folder = root.join(id);
    let file = std::fs::read_dir(&folder)
        .ok()
        .and_then(|entries| entries.flatten().map(|e| e.path()).find(|p| p.is_file()));
    file.ok_or_else(|| Problem::status(404, "That upload was not found. It may have expired; upload the file again."))
}

pub fn delete_upload(root: &Path, id: &str) {
    if valid_id(id) {
        let _ = std::fs::remove_dir_all(root.join(id));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize() {
        assert_eq!(sanitize_filename(Some("../../etc/pass wd.csv")), "pass wd.csv");
        assert_eq!(sanitize_filename(Some("")), "upload");
        assert_eq!(sanitize_filename(None), "upload");
        assert_eq!(sanitize_filename(Some("a/b\\c<>.txt")), "c_.txt");
        assert_eq!(sanitize_filename(Some(" .hidden. ")), "hidden");
        let long = format!("{}.jsonl", "x".repeat(200));
        assert_eq!(sanitize_filename(Some(&long)), format!("{}.jsonl", "x".repeat(100)));
        assert_eq!(sanitize_filename(Some("Données été.csv")), "Données été.csv");
    }

    #[test]
    fn cleanup() {
        let dir = tempfile::tempdir().unwrap();
        let old = dir.path().join("old");
        let new = dir.path().join("new");
        std::fs::create_dir(&old).unwrap();
        std::fs::create_dir(&new).unwrap();
        std::fs::write(old.join("f"), "x").unwrap();
        let t = SystemTime::now() - Duration::from_secs(48 * 3600);
        std::fs::File::open(&old).unwrap().set_modified(t).unwrap();
        assert_eq!(cleanup_old_uploads(dir.path(), UPLOAD_MAX_AGE), 1);
        assert!(!old.exists() && new.exists());
    }

    #[test]
    fn ids() {
        assert!(valid_id(&"a".repeat(32)));
        assert!(!valid_id("../x"));
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(upload_file(dir.path(), &"0".repeat(32)).unwrap_err().status, 404);
    }
}
