//! Downloads: the export dialog's `<a href download>` links. WKWebView, WebView2 and WebKitGTK
//! hand them to the app; each is saved in the Downloads folder under the file name the server
//! sent (Content-Disposition), never over an existing file, then shown in Finder (Explorer, the
//! file manager).

use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};

use tauri::webview::DownloadEvent;
use tauri::{Manager, Runtime, Webview};

/// Downloads in progress: (URL, destination). macOS does not report the path when a download
/// finishes, so it is looked up here.
#[derive(Default)]
pub struct Downloads(Mutex<Vec<(String, PathBuf)>>);

impl Downloads {
    fn lock(&self) -> MutexGuard<'_, Vec<(String, PathBuf)>> {
        self.0.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// The destination recorded for `url` (the oldest one if the same URL is downloading twice).
    fn take(&self, url: &str) -> Option<PathBuf> {
        let mut pending = self.lock();
        let i = pending.iter().position(|(u, _)| u == url)?;
        Some(pending.remove(i).1)
    }
}

pub fn handle<R: Runtime>(webview: Webview<R>, event: DownloadEvent<'_>) -> bool {
    let app = webview.app_handle();
    let pending = app.state::<Downloads>();
    match event {
        DownloadEvent::Requested { url, destination } => {
            // The webview proposes Downloads/<server's file name>, with " (1)" added when that
            // file exists (wry puts it before the first dot). Recompute it so the name stays the
            // server's and the counter goes before the extension.
            let dir = app.path().download_dir().ok().or_else(|| destination.parent().map(Path::to_path_buf));
            let name = destination.file_name().and_then(|n| n.to_str()).map(str::to_string);
            if let (Some(dir), Some(name)) = (dir, name) {
                if let Err(e) = std::fs::create_dir_all(&dir) {
                    tracing::warn!(dir = %dir.display(), error = %e, "could not create the Downloads folder");
                }
                let name = original_name(&dir, &name);
                *destination = free_path(&dir, &name);
            }
            tracing::info!(file = %destination.display(), "saving a download");
            pending.lock().push((url.to_string(), destination.clone()));
            true
        }
        DownloadEvent::Finished { url, path, success } => {
            let recorded = pending.take(url.as_str());
            let path = path.filter(|p| !p.as_os_str().is_empty()).or(recorded);
            match (success, path) {
                (true, Some(path)) => {
                    tracing::info!(file = %path.display(), "download saved");
                    if let Err(e) = tauri_plugin_opener::reveal_item_in_dir(&path) {
                        tracing::debug!(error = %e, "could not show the download in the file manager");
                    }
                }
                (true, None) => tracing::info!("download saved"),
                (false, path) => tracing::warn!(
                    file = %path.map(|p| p.display().to_string()).unwrap_or_default(),
                    "download failed"
                ),
            }
            true
        }
        _ => true,
    }
}

/// Undo a webview's " (n)" counter: when `name` is `<a> (n)<b>` and `<a><b>` exists in `dir`,
/// the webview added the counter because `<a><b>` was taken, so `<a><b>` is the server's name.
/// Handles both wry's "qwen3 (1).5-sft.jsonl" and WebView2's "qwen3.5-sft (1).jsonl".
fn original_name(dir: &Path, name: &str) -> String {
    let mut from = 0;
    while let Some(i) = name[from..].find(" (").map(|i| i + from) {
        let rest = &name[i + 2..];
        let digits = rest.bytes().take_while(u8::is_ascii_digit).count();
        if digits > 0 && rest[digits..].starts_with(')') {
            let tail = &rest[digits + 1..];
            if tail.is_empty() || tail.starts_with('.') {
                let candidate = format!("{}{tail}", &name[..i]);
                if dir.join(&candidate).exists() {
                    return candidate;
                }
            }
        }
        from = i + 2;
    }
    name.to_string()
}

/// `dir/name` if it is free, else `dir/<stem> (n).<ext>` with the first free n.
fn free_path(dir: &Path, name: &str) -> PathBuf {
    let path = dir.join(name);
    if !path.exists() {
        return path;
    }
    let (stem, ext) = match name.rfind('.') {
        Some(i) if i > 0 => name.split_at(i),
        _ => (name, ""),
    };
    (1u32..)
        .map(|n| dir.join(format!("{stem} ({n}){ext}")))
        .find(|p| !p.exists())
        .unwrap_or(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn touch(dir: &Path, name: &str) {
        std::fs::write(dir.join(name), b"").unwrap();
    }

    fn file_name(p: PathBuf) -> String {
        p.file_name().unwrap().to_str().unwrap().to_string()
    }

    #[test]
    fn free_name_is_kept() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(file_name(free_path(dir.path(), "run-sft.jsonl")), "run-sft.jsonl");
    }

    #[test]
    fn counter_goes_before_the_last_extension() {
        let dir = tempfile::tempdir().unwrap();
        touch(dir.path(), "qwen3.5-run-sft.jsonl");
        assert_eq!(file_name(free_path(dir.path(), "qwen3.5-run-sft.jsonl")), "qwen3.5-run-sft (1).jsonl");
        touch(dir.path(), "qwen3.5-run-sft (1).jsonl");
        assert_eq!(file_name(free_path(dir.path(), "qwen3.5-run-sft.jsonl")), "qwen3.5-run-sft (2).jsonl");
    }

    #[test]
    fn name_without_extension() {
        let dir = tempfile::tempdir().unwrap();
        touch(dir.path(), "export");
        assert_eq!(file_name(free_path(dir.path(), "export")), "export (1)");
    }

    #[test]
    fn wry_counter_is_undone() {
        let dir = tempfile::tempdir().unwrap();
        touch(dir.path(), "qwen3.5-run-sft.jsonl");
        assert_eq!(original_name(dir.path(), "qwen3 (1).5-run-sft.jsonl"), "qwen3.5-run-sft.jsonl");
    }

    #[test]
    fn webview2_counter_is_undone() {
        let dir = tempfile::tempdir().unwrap();
        touch(dir.path(), "qwen3.5-run-sft.jsonl");
        assert_eq!(original_name(dir.path(), "qwen3.5-run-sft (1).jsonl"), "qwen3.5-run-sft.jsonl");
    }

    #[test]
    fn a_real_name_with_a_counter_is_kept() {
        // "draft (2).jsonl" is the server's name when "draft.jsonl" does not exist.
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(original_name(dir.path(), "draft (2).jsonl"), "draft (2).jsonl");
    }
}
