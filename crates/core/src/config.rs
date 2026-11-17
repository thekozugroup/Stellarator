use std::path::PathBuf;

/// Where Stellarator runs. Changes only defaults (the presets' base URLs, the bind address).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    /// The Docker image: model servers on the host are at host.docker.internal.
    Docker,
    /// The desktop app: bound to 127.0.0.1 behind a per-launch token.
    Desktop,
    /// `stellarator-server` run by hand.
    Native,
}

#[derive(Clone, Debug)]
pub struct Config {
    pub data_dir: PathBuf,
    pub mode: Mode,
    /// Hugging Face token from the environment (HF_TOKEN or HUGGING_FACE_HUB_TOKEN). Wins over the
    /// one saved in settings.
    pub env_hf_token: Option<String>,
    pub hf_datasets_server: String,
    pub hf_hub: String,
    /// Desktop only: requests must carry the cookie set from this token (see the server crate).
    pub launch_token: Option<String>,
    pub version: &'static str,
}

impl Config {
    /// Read the environment. `STELLARATOR_DATA_DIR` defaults to `./data`.
    pub fn from_env() -> Self {
        let data_dir = std::env::var("STELLARATOR_DATA_DIR").unwrap_or_else(|_| "./data".into());
        let mode = if std::env::var("STELLARATOR_DOCKER").is_ok_and(|v| v == "1") {
            Mode::Docker
        } else {
            Mode::Native
        };
        Self::new(PathBuf::from(data_dir), mode)
    }

    pub fn new(data_dir: PathBuf, mode: Mode) -> Self {
        let data_dir = std::fs::canonicalize(&data_dir).unwrap_or(data_dir);
        let env_hf_token = std::env::var("HF_TOKEN")
            .ok()
            .or_else(|| std::env::var("HUGGING_FACE_HUB_TOKEN").ok())
            .filter(|t| !t.trim().is_empty());
        Self {
            data_dir,
            mode,
            env_hf_token,
            hf_datasets_server: std::env::var("STELLARATOR_HF_DATASETS_SERVER")
                .unwrap_or_else(|_| "https://datasets-server.huggingface.co".into()),
            hf_hub: std::env::var("STELLARATOR_HF_HUB").unwrap_or_else(|_| "https://huggingface.co".into()),
            launch_token: None,
            version: env!("CARGO_PKG_VERSION"),
        }
    }

    pub fn db_path(&self) -> PathBuf {
        self.data_dir.join("stellarator.db")
    }

    /// Uploaded files waiting to be imported.
    pub fn upload_dir(&self) -> PathBuf {
        self.data_dir.join("uploads")
    }

    /// Temporary export files (Parquet and JSON are written whole, then streamed).
    pub fn export_dir(&self) -> PathBuf {
        self.data_dir.join("exports")
    }

    /// How a model server on this machine is reached from Stellarator.
    pub fn local_host(&self) -> &'static str {
        match self.mode {
            Mode::Docker => "host.docker.internal",
            _ => "localhost",
        }
    }
}
