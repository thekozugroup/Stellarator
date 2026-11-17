//! `stellarator-mock`: a fake OpenAI-compatible server for trying Stellarator without a GPU.
//!
//!     stellarator-mock --port 9100 --tps 60
//!
//! Then add an endpoint with base URL http://localhost:9100/v1. See the library docs for the models.

use std::net::{IpAddr, SocketAddr};

const USAGE: &str = "Usage: stellarator-mock [--host 0.0.0.0] [--port 9100] [--tps 60]";

fn main() {
    let mut host: IpAddr = IpAddr::from([0, 0, 0, 0]);
    let mut port: u16 = 9100;
    let mut tps: f64 = 60.0;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        let (name, inline) = match arg.split_once('=') {
            Some((n, v)) => (n.to_string(), Some(v.to_string())),
            None => (arg.clone(), None),
        };
        if name == "-h" || name == "--help" {
            println!("{USAGE}");
            return;
        }
        let value = inline.or_else(|| args.next());
        let ok = match (name.as_str(), value) {
            ("--host", Some(v)) => v.parse().map(|v| host = v).is_ok(),
            ("--port", Some(v)) => v.parse().map(|v| port = v).is_ok(),
            ("--tps", Some(v)) => v.parse::<f64>().ok().filter(|t| *t > 0.0).map(|v| tps = v).is_some(),
            _ => false,
        };
        if !ok {
            eprintln!("Couldn't read the argument {arg:?}.\n{USAGE}");
            std::process::exit(2);
        }
    }
    let runtime = match tokio::runtime::Runtime::new() {
        Ok(r) => r,
        Err(e) => {
            eprintln!("Couldn't start: {e}");
            std::process::exit(1);
        }
    };
    runtime.block_on(async move {
        match stellarator_mock::serve(SocketAddr::new(host, port), tps).await {
            Ok((addr, join)) => {
                eprintln!("Mock OpenAI-compatible server on http://{addr}/v1 ({tps} tokens/s per sequence)");
                tokio::select! {
                    _ = join => {}
                    _ = tokio::signal::ctrl_c() => {}
                }
            }
            Err(e) => {
                eprintln!("Couldn't listen on {host}:{port}: {e}");
                std::process::exit(1);
            }
        }
    });
}
