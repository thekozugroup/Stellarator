use stellarator_supervisor::config::Config;
use stellarator_supervisor::cost::cost_for;

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
fn h100_full_hour_one_gpu() {
    let v = cost_for(&cfg(), "H100", 1, 3600.0);
    assert!((v - 4.50).abs() < 1e-9);
}

#[test]
fn h100_lower_case_matches() {
    let v = cost_for(&cfg(), "h100", 1, 3600.0);
    assert!((v - 4.50).abs() < 1e-9);
}

#[test]
fn a100_eight_gpus_quarter_hour() {
    // 0.25h * 8 * 2.20 = 4.40
    let v = cost_for(&cfg(), "A100", 8, 900.0);
    assert!((v - 4.40).abs() < 1e-9);
}

#[test]
fn linear_in_seconds() {
    let a = cost_for(&cfg(), "H100", 1, 1800.0);
    let b = cost_for(&cfg(), "H100", 1, 3600.0);
    assert!((b - 2.0 * a).abs() < 1e-9);
}

#[test]
fn unknown_gpu_falls_back_to_h100() {
    let v = cost_for(&cfg(), "B200", 1, 3600.0);
    assert!((v - 4.50).abs() < 1e-9);
}
