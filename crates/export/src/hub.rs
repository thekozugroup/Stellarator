//! Pushing a dataset to the Hugging Face Hub over its HTTP API: create the repo, then commit the
//! Parquet file and the dataset card (files over ~10 MB go through the LFS batch API).

use std::path::Path;
use std::time::Duration;

use base64::Engine;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use stellarator_core::schemas::{ExportOptions, PrefExportOptions};
use stellarator_core::{ApiError, Ctx};
use tokio::io::AsyncReadExt;
use tokio_util::io::ReaderStream;

use crate::records::{placement_name, pref_name, sft_name, RunInfo};

pub const HF_TOKEN_MISSING: &str = "Add a Hugging Face token in Settings to push datasets.";
const INLINE_LIMIT: u64 = 10 * 1024 * 1024;
const B64: base64::engine::GeneralPurpose = base64::engine::general_purpose::STANDARD;

// ------------------------------------------------------------------------------ dataset card

fn py_value(v: &Value) -> String {
    match v {
        Value::Bool(b) => if *b { "True" } else { "False" }.into(),
        Value::Number(n) => match n.as_f64() {
            // Python prints 1.0, Rust would print 1.
            Some(f) if n.is_f64() && f.fract() == 0.0 && f.abs() < 1e16 => format!("{f:.1}"),
            _ => n.to_string(),
        },
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

pub fn param_line(params: &Value) -> String {
    let mut parts = Vec::new();
    for k in ["samples_per_prompt", "temperature", "top_p", "max_tokens", "seed"] {
        if let Some(v) = params.get(k).filter(|v| !v.is_null()) {
            parts.push(format!("{k}={}", py_value(v)));
        }
    }
    if params.get("system_prompt").and_then(Value::as_str).is_some_and(|s| !s.is_empty()) {
        parts.push("system prompt set".into());
    }
    if params.get("user_template").and_then(Value::as_str).is_some_and(|s| !s.is_empty()) {
        parts.push("user template set".into());
    }
    if parts.is_empty() {
        "defaults".into()
    } else {
        parts.join(", ")
    }
}

fn card(title: &str, split: &str, lines: &[String]) -> String {
    let mut s = format!(
        "---\nconfigs:\n- config_name: default\n  data_files:\n  - split: {split}\n    path: data/{split}-*\n---\n\n# {title}\n\n"
    );
    s.push_str(&lines.iter().map(|l| format!("- {l}")).collect::<Vec<_>>().join("\n"));
    s.push('\n');
    s
}

/// "Tools: lookup, add (MCP servers: docs)" for a run with tools; in code mode, the code tool and
/// the methods its code calls.
pub fn tools_line(run: &RunInfo) -> Option<String> {
    let t = run.tools.as_ref()?;
    let names: Vec<String> = t.servers.iter().flat_map(|s| s.tools.iter().map(|x| x.name.clone())).collect();
    let servers: Vec<&str> = t.servers.iter().map(|s| s.name.as_str()).collect();
    if let Some(code) = &t.code {
        return Some(format!(
            "Tools: Code Mode ({}): one tool, `{}`, whose JavaScript calls {} (MCP servers: {})",
            code.format,
            code.tool_name,
            names.join(", "),
            servers.join(", ")
        ));
    }
    Some(format!("Tools: {} (MCP servers: {})", names.join(", "), servers.join(", ")))
}

pub fn run_card(run: &RunInfo, prompt_set_name: &str, count: i64, o: &ExportOptions, split: &str) -> String {
    let mut lines = vec![
        format!("Model: {}", run.model),
        format!("Endpoint: {}", run.endpoint_name),
        format!("Prompt set: {prompt_set_name}"),
        format!("Records: {count}"),
        format!("Format: {} (reasoning: {})", sft_name(o.format), placement_name(o.reasoning)),
        format!("Generation parameters: {}", param_line(&run.params)),
    ];
    lines.extend(tools_line(run));
    lines.push("Generated with Stellarator.".into());
    card(&run.name, split, &lines)
}

pub fn pref_card(
    set_name: &str,
    prompt_set_name: &str,
    runs: &[RunInfo],
    count: i64,
    o: &PrefExportOptions,
    split: &str,
) -> String {
    let uniq = |f: &dyn Fn(&RunInfo) -> String| {
        let mut v: Vec<String> = runs.iter().map(f).collect();
        v.sort();
        v.dedup();
        if v.is_empty() {
            "unknown".to_string()
        } else {
            v.join(", ")
        }
    };
    let params = if runs.is_empty() {
        "unknown".to_string()
    } else {
        runs.iter().map(|r| format!("{}: {}", r.name, param_line(&r.params))).collect::<Vec<_>>().join("; ")
    };
    let kind = if o.conversational { "conversational" } else { "text" };
    card(
        set_name,
        split,
        &[
            format!("Models: {}", uniq(&|r| r.model.clone())),
            format!("Endpoints: {}", uniq(&|r| r.endpoint_name.clone())),
            format!("Prompt set: {prompt_set_name}"),
            format!("Records: {count}"),
            format!("Format: {}, {kind} (reasoning: {})", pref_name(o.format), placement_name(o.reasoning)),
            format!("Generation parameters: {params}"),
            "Generated with Stellarator.".into(),
        ],
    )
}

// ------------------------------------------------------------------------------ errors

pub fn status_error(status: u16, repo_id: &str) -> ApiError {
    match status {
        401 => ApiError::bad_request("Hugging Face rejected the token. Check it in Settings."),
        403 => ApiError::bad_request(format!(
            "The token cannot write to {repo_id}. Use a token with write access to that account."
        )),
        404 => ApiError::bad_request(format!(
            "Hugging Face could not find {repo_id}. Check the owner name in the repo id."
        )),
        409 => ApiError::conflict(format!("{repo_id} already exists in a way that conflicts. Pick another repo id.")),
        other => ApiError::new(
            axum::http::StatusCode::BAD_GATEWAY,
            format!("Hugging Face returned an error ({other}). Try again in a moment."),
        ),
    }
}

fn network_error(e: reqwest::Error) -> ApiError {
    tracing::warn!(error = %e, "Hugging Face request failed");
    ApiError::new(
        axum::http::StatusCode::BAD_GATEWAY,
        "Could not reach Hugging Face. Check the network connection and try again.",
    )
}

/// owner/name or name, letters, digits, `_`, `-` and `.`; at most one slash; no `--` or `..`.
pub fn valid_repo_id(id: &str) -> bool {
    let ok_part = |p: &str| {
        !p.is_empty()
            && p.len() <= 96
            && p.chars().all(|c| c.is_alphanumeric() || matches!(c, '_' | '-' | '.'))
            && p.chars().next().is_some_and(|c| c.is_alphanumeric() || c == '_')
            && p.chars().last().is_some_and(|c| c.is_alphanumeric() || c == '_')
    };
    if id.contains("--") || id.contains("..") || id.ends_with(".git") {
        return false;
    }
    let parts: Vec<&str> = id.split('/').collect();
    match parts.as_slice() {
        [name] => ok_part(name),
        [owner, name] => ok_part(owner) && ok_part(name),
        _ => false,
    }
}

pub fn valid_split(split: &str) -> bool {
    !split.is_empty() && split.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '-'))
}

// ------------------------------------------------------------------------------ client

struct Hub<'a> {
    ctx: &'a Ctx,
    base: String,
    token: &'a str,
    /// The repo id as the user typed it, for error sentences.
    shown: String,
}

const LONG: Duration = Duration::from_secs(3600);

impl Hub<'_> {
    fn post(&self, url: String) -> reqwest::RequestBuilder {
        self.ctx.http.post(url).bearer_auth(self.token)
    }

    async fn check(&self, r: reqwest::Result<reqwest::Response>) -> Result<reqwest::Response, ApiError> {
        let r = r.map_err(network_error)?;
        if r.status().is_success() {
            Ok(r)
        } else {
            let status = r.status().as_u16();
            let body = r.text().await.unwrap_or_default();
            tracing::warn!(status, body = %body.chars().take(300).collect::<String>(), "Hugging Face refused a request");
            Err(status_error(status, &self.shown))
        }
    }

    async fn create_repo(&self, repo_id: &str, private: bool) -> Result<(), ApiError> {
        let mut body = json!({"type": "dataset", "private": private});
        match repo_id.split_once('/') {
            Some((org, name)) => {
                body["name"] = name.into();
                body["organization"] = org.into();
            }
            None => body["name"] = repo_id.into(),
        }
        let r = self.post(format!("{}/api/repos/create", self.base)).json(&body).send().await;
        match r {
            // 409 means it exists already, which is fine.
            Ok(resp) if resp.status().as_u16() == 409 => Ok(()),
            other => self.check(other).await.map(|_| ()),
        }
    }

    /// "owner/name" for a bare name: the token's own account.
    async fn qualified(&self, repo_id: &str) -> Result<String, ApiError> {
        if repo_id.contains('/') {
            return Ok(repo_id.to_string());
        }
        let r = self.ctx.http.get(format!("{}/api/whoami-v2", self.base)).bearer_auth(self.token).send().await;
        let v: Value = self.check(r).await?.json().await.map_err(network_error)?;
        let user = v.get("name").and_then(Value::as_str).unwrap_or_default();
        Ok(format!("{user}/{repo_id}"))
    }

    async fn upload_mode(&self, repo: &str, path_in_repo: &str, size: u64, sample: &[u8]) -> Result<String, ApiError> {
        let body = json!({"files": [{"path": path_in_repo, "sample": B64.encode(sample), "size": size}]});
        let r = self.post(format!("{}/api/datasets/{repo}/preupload/main", self.base)).json(&body).send().await;
        let v: Value = self.check(r).await?.json().await.map_err(network_error)?;
        Ok(v.pointer("/files/0/uploadMode").and_then(Value::as_str).unwrap_or("regular").to_string())
    }

    async fn commit(&self, repo: &str, summary: &str, file_line: Value) -> Result<(), ApiError> {
        let header = json!({"key": "header", "value": {"summary": summary, "description": ""}});
        let body = format!("{header}\n{file_line}\n");
        let r = self
            .post(format!("{}/api/datasets/{repo}/commit/main", self.base))
            .header("content-type", "application/x-ndjson")
            .timeout(LONG)
            .body(body)
            .send()
            .await;
        self.check(r).await.map(|_| ())
    }

    /// Upload a small in-memory file (the dataset card).
    async fn upload_bytes(&self, repo: &str, path_in_repo: &str, data: &[u8], summary: &str) -> Result<(), ApiError> {
        let mode = self.upload_mode(repo, path_in_repo, data.len() as u64, &data[..data.len().min(512)]).await?;
        if mode == "lfs" || data.len() as u64 > INLINE_LIMIT {
            return Err(ApiError::internal("The dataset card was too large to upload."));
        }
        let line = json!({"key": "file", "value": {"content": B64.encode(data), "path": path_in_repo, "encoding": "base64"}});
        self.commit(repo, summary, line).await
    }

    /// Upload a file from disk: inline when small, LFS when large or the hub asks for it.
    async fn upload_file(&self, repo: &str, path_in_repo: &str, file: &Path, summary: &str) -> Result<(), ApiError> {
        let io = |e: std::io::Error| ApiError::internal(format!("Could not read the export file: {e}"));
        let size = tokio::fs::metadata(file).await.map_err(io)?.len();
        let mut head = vec![0u8; 512.min(size as usize)];
        tokio::fs::File::open(file).await.map_err(io)?.read_exact(&mut head).await.map_err(io)?;
        let mode = self.upload_mode(repo, path_in_repo, size, &head).await?;
        if mode != "lfs" && size <= INLINE_LIMIT {
            let data = tokio::fs::read(file).await.map_err(io)?;
            let line = json!({"key": "file", "value": {"content": B64.encode(data), "path": path_in_repo, "encoding": "base64"}});
            return self.commit(repo, summary, line).await;
        }
        let oid = sha256_file(file).await.map_err(io)?;
        self.lfs_upload(repo, file, &oid, size).await?;
        let line = json!({"key": "lfsFile", "value": {"path": path_in_repo, "algo": "sha256", "oid": oid, "size": size}});
        self.commit(repo, summary, line).await
    }

    async fn lfs_upload(&self, repo: &str, file: &Path, oid: &str, size: u64) -> Result<(), ApiError> {
        let io = |e: std::io::Error| ApiError::internal(format!("Could not read the export file: {e}"));
        let lfs = "application/vnd.git-lfs+json";
        let body = json!({
            "operation": "upload",
            "transfers": ["basic"],
            "objects": [{"oid": oid, "size": size}],
            "hash_algo": "sha256",
        });
        let r = self
            .post(format!("{}/datasets/{repo}.git/info/lfs/objects/batch", self.base))
            .header("accept", lfs)
            .header("content-type", lfs)
            .body(body.to_string())
            .send()
            .await;
        let v: Value = self.check(r).await?.json().await.map_err(network_error)?;
        let obj = v.pointer("/objects/0").cloned().unwrap_or(Value::Null);
        if obj.get("error").is_some() {
            tracing::warn!(error = %obj["error"], "Hugging Face LFS refused an object");
            return Err(status_error(500, &self.shown));
        }
        let Some(actions) = obj.get("actions").filter(|a| a.is_object()) else {
            return Ok(()); // already stored
        };
        if let Some(up) = actions.get("upload") {
            let href = up.get("href").and_then(Value::as_str).unwrap_or_default().to_string();
            let mut req = self.ctx.http.put(href).timeout(LONG).header("content-length", size);
            for (k, v) in up.get("header").and_then(Value::as_object).into_iter().flatten() {
                if let Some(v) = v.as_str() {
                    req = req.header(k.as_str(), v);
                }
            }
            let f = tokio::fs::File::open(file).await.map_err(io)?;
            let r = req.body(reqwest::Body::wrap_stream(ReaderStream::with_capacity(f, 1 << 20))).send().await;
            self.check(r).await?;
        }
        if let Some(ver) = actions.get("verify") {
            let href = ver.get("href").and_then(Value::as_str).unwrap_or_default().to_string();
            let mut req = self.ctx.http.post(href).bearer_auth(self.token).json(&json!({"oid": oid, "size": size}));
            for (k, v) in ver.get("header").and_then(Value::as_object).into_iter().flatten() {
                if let Some(v) = v.as_str() {
                    req = req.header(k.as_str(), v);
                }
            }
            self.check(req.send().await).await?;
        }
        Ok(())
    }
}

async fn sha256_file(path: &Path) -> std::io::Result<String> {
    let mut f = tokio::fs::File::open(path).await?;
    let mut h = Sha256::new();
    let mut buf = vec![0u8; 1 << 20];
    loop {
        let n = f.read(&mut buf).await?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
    }
    Ok(h.finalize().iter().map(|b| format!("{b:02x}")).collect())
}

/// Create the dataset repo (if needed) and commit the Parquet file and README. Returns the
/// dataset's page URL.
pub async fn push(
    ctx: &Ctx,
    token: &str,
    repo_id: &str,
    private: bool,
    split: &str,
    parquet: &Path,
    card: &str,
) -> Result<String, ApiError> {
    let base = ctx.config.hf_hub.trim_end_matches('/').to_string();
    let hub = Hub { ctx, base: base.clone(), token, shown: repo_id.to_string() };
    hub.create_repo(repo_id, private).await?;
    let repo = hub.qualified(repo_id).await?;
    hub.upload_file(&repo, &format!("data/{split}-00000-of-00001.parquet"), parquet, "Add data from Stellarator").await?;
    hub.upload_bytes(&repo, "README.md", card.as_bytes(), "Add dataset card").await?;
    Ok(format!("{base}/datasets/{repo}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repo_ids() {
        assert!(valid_repo_id("me/ds"));
        assert!(valid_repo_id("a.b/c_d-e"));
        assert!(!valid_repo_id("a/b/c"));
        assert!(!valid_repo_id("me/"));
        assert!(!valid_repo_id("me/d s"));
        assert!(!valid_repo_id("-me/ds"));
        assert!(!valid_repo_id("me/a..b"));
    }

    #[test]
    fn param_lines() {
        let p = json!({"samples_per_prompt": 2, "temperature": 1.0, "top_p": null, "max_tokens": 512, "system_prompt": "x"});
        assert_eq!(param_line(&p), "samples_per_prompt=2, temperature=1.0, max_tokens=512, system prompt set");
        assert_eq!(param_line(&json!({})), "defaults");
    }
}
