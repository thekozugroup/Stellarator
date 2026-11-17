//! Starting points for the Add endpoint form. Base URLs use the right host for where Stellarator
//! runs: `localhost` for the desktop app and native builds, `host.docker.internal` in Docker.

use std::collections::BTreeMap;

use crate::schemas::{EndpointPreset, Location};
use crate::Config;

pub fn endpoint_presets(config: &Config) -> Vec<EndpointPreset> {
    let host = config.local_host();
    let local = |id, name, port: u16, seqs| EndpointPreset {
        id,
        name,
        base_url: format!("http://{host}:{port}/v1"),
        location: Location::Local,
        default_concurrency: seqs,
        needs_api_key: false,
        api_key_url: None,
        headers: BTreeMap::new(),
    };
    let mut openrouter_headers = BTreeMap::new();
    // OpenRouter attributes requests to an app by these headers (optional, harmless elsewhere).
    openrouter_headers.insert("X-Title".to_string(), "Stellarator".to_string());
    vec![
        EndpointPreset {
            id: "openrouter",
            name: "OpenRouter",
            base_url: "https://openrouter.ai/api/v1".into(),
            location: Location::Cloud,
            default_concurrency: 8,
            needs_api_key: true,
            api_key_url: Some("https://openrouter.ai/settings/keys"),
            headers: openrouter_headers,
        },
        // LM Studio's local server (Developer tab > Start server), default port 1234. It runs one
        // request at a time per loaded model unless parallel requests are enabled.
        local("lmstudio", "LM Studio", 1234, 4),
        // Ollama's OpenAI-compatible API, default port 11434. Parallelism follows
        // OLLAMA_NUM_PARALLEL on the Ollama side.
        local("ollama", "Ollama", 11434, 4),
        local("vllm", "vLLM", 8000, 64),
        local("sglang", "SGLang", 30000, 64),
        local("llamacpp", "llama.cpp server", 8080, 4),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Mode;

    #[test]
    fn hosts_follow_mode() {
        let mut c = Config::new(std::env::temp_dir(), Mode::Native);
        let p = endpoint_presets(&c);
        assert_eq!(p.iter().find(|p| p.id == "ollama").unwrap().base_url, "http://localhost:11434/v1");
        c.mode = Mode::Docker;
        let p = endpoint_presets(&c);
        assert_eq!(p.iter().find(|p| p.id == "lmstudio").unwrap().base_url, "http://host.docker.internal:1234/v1");
        assert_eq!(p[0].base_url, "https://openrouter.ai/api/v1");
    }
}
