mod common;

use axum::http::StatusCode;
use common::*;
use serde_json::json;

#[tokio::test]
async fn health_and_settings_round_trip() {
    let env = env().await;
    let (status, health) = env.get("/api/health").await;
    assert_eq!((status, health["ok"].clone()), (StatusCode::OK, json!(true)));

    let (status, s) = env.get("/api/settings").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(s["mode"], "native");
    assert_eq!(s["version"], env!("CARGO_PKG_VERSION"));
    assert_eq!(s["data_dir"], env.ctx.config.data_dir.display().to_string());
    if !s["hf_token_from_env"].as_bool().unwrap() {
        assert_eq!(s["hf_token_set"], false);
        assert_eq!(s["hf_token_hint"], json!(null));

        // Set (trimmed), keep with an empty body, remove with "".
        let (_, s) = env.put("/api/settings", json!({"hf_token": "  hf_abcdefghijklmnop  "})).await;
        assert_eq!((s["hf_token_set"].clone(), s["hf_token_hint"].clone()), (json!(true), json!("…mnop")));
        let (_, s) = env.put("/api/settings", json!({})).await;
        assert_eq!(s["hf_token_set"], true);
        let (_, s) = env.put("/api/settings", json!({"hf_token": null})).await;
        assert_eq!(s["hf_token_set"], true);
        let (_, s) = env.put("/api/settings", json!({"hf_token": "short"})).await;
        assert_eq!(s["hf_token_hint"], "…");
        let (_, s) = env.put("/api/settings", json!({"hf_token": ""})).await;
        assert_eq!((s["hf_token_set"].clone(), s["hf_token_hint"].clone()), (json!(false), json!(null)));
        let (_, s) = env.put("/api/settings", json!({"hf_token": "   "})).await;
        assert_eq!(s["hf_token_set"], false);
        assert_eq!(env.count("SELECT COUNT(*) FROM settings").await, 0);
    }
}

#[tokio::test]
async fn mode_follows_the_config() {
    use std::sync::Arc;
    use stellarator_core::config::Mode;
    use stellarator_core::probes::Idle;
    use stellarator_server::{AppState, Parts};

    for (mode, name) in [(Mode::Docker, "docker"), (Mode::Desktop, "desktop"), (Mode::Native, "native")] {
        let dir = tempfile::tempdir().unwrap();
        let db = stellarator_core::db::connect_memory().await.unwrap();
        let ctx = stellarator_core::Ctx {
            db,
            config: Arc::new(stellarator_core::Config::new(dir.path().to_path_buf(), mode)),
            http: reqwest::Client::new(),
        };
        let parts = Parts { state: AppState { ctx, runs: Arc::new(Idle), imports: Arc::new(Idle) }, extra: vec![], lifecycle: vec![] };
        let env = Env { ctx: parts.state.ctx.clone(), app: stellarator_server::app(&parts), dir };
        assert_eq!(env.get("/api/settings").await.1["mode"], name);
        let presets = env.get("/api/endpoint-presets").await.1;
        let host = if mode == Mode::Docker { "host.docker.internal" } else { "localhost" };
        assert_eq!(presets[1]["base_url"], format!("http://{host}:1234/v1"));
    }
}
