use crate::config::Config;

/// Compute cost in USD given gpu type, gpu count, and elapsed gpu-seconds (per GPU).
/// gpu_seconds is total wall-clock seconds the job has been active; cost = gpu_seconds * gpu_count * (rate / 3600).
pub fn cost_for(cfg: &Config, gpu_type: &str, gpu_count: i64, gpu_seconds: f64) -> f64 {
    let rate = match gpu_type.to_ascii_uppercase().as_str() {
        "H100" => cfg.cost_h100_per_hour,
        "A100" => cfg.cost_a100_per_hour,
        _ => cfg.cost_h100_per_hour, // safe default
    };
    (gpu_seconds / 3600.0) * (gpu_count.max(1) as f64) * rate
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;

    fn cfg() -> Config {
        Config {
            tinker_api_key: "x".into(),
            tinker_base_url: "http://x".into(),
            db_url: ":memory:".into(),
            bind_addr: "0".into(),
            poll_interval_secs: 5,
            cost_h100_per_hour: 4.50,
            cost_a100_per_hour: 2.20,
        }
    }

    #[test]
    fn h100_one_hour_one_gpu() {
        let c = cost_for(&cfg(), "H100", 1, 3600.0);
        assert!((c - 4.50).abs() < 1e-9);
    }

    #[test]
    fn a100_two_gpus_half_hour() {
        let c = cost_for(&cfg(), "A100", 2, 1800.0);
        // 0.5h * 2 * 2.20 = 2.20
        assert!((c - 2.20).abs() < 1e-9);
    }

    #[test]
    fn unknown_gpu_defaults_to_h100() {
        let c = cost_for(&cfg(), "B200", 1, 3600.0);
        assert!((c - 4.50).abs() < 1e-9);
    }

    #[test]
    fn zero_gpu_count_treated_as_one() {
        let c = cost_for(&cfg(), "H100", 0, 3600.0);
        assert!((c - 4.50).abs() < 1e-9);
    }
}
