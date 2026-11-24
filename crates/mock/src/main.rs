//! `stellarator-mock`: a fake OpenAI-compatible server for trying Stellarator without a GPU.
//!
//!     stellarator-mock --port 9100 --tps 60 [--api-key sk-mock]
//!
//! Then add an endpoint with base URL http://localhost:9100/v1. See the library docs for the models.
//! With `--api-key`, requests without `Authorization: Bearer <key>` get 401 (a rejected key).
//!
//! The same port serves a small MCP server at http://localhost:9100/mcp (streamable HTTP; behind
//! the key too when one is set). `stellarator-mock mcp-stdio` serves its tools on stdin and stdout
//! instead, as a stdio MCP server. A second MCP server, an issue tracker, is at
//! http://localhost:9100/tracker/mcp.

use std::net::{IpAddr, SocketAddr};

const USAGE: &str =
    "Usage: stellarator-mock [--host 0.0.0.0] [--port 9100] [--tps 60] [--api-key KEY]\n       stellarator-mock mcp-stdio";

fn main() {
    if std::env::args().nth(1).as_deref() == Some("mcp-stdio") {
        let runtime = tokio::runtime::Runtime::new().expect("tokio runtime");
        if let Err(e) = runtime.block_on(stellarator_mock::mcp::serve_stdio()) {
            eprintln!("The MCP server stopped: {e}");
            std::process::exit(1);
        }
        return;
    }
    let mut host: IpAddr = IpAddr::from([0, 0, 0, 0]);
    let mut port: u16 = 9100;
    let mut tps: f64 = 60.0;
    let mut api_key: Option<String> = None;
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
            ("--api-key", Some(v)) => {
                api_key = Some(v);
                true
            }
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
        let keyed = api_key.is_some();
        match stellarator_mock::serve_with_key(SocketAddr::new(host, port), tps, api_key).await {
            Ok((addr, join)) => {
                let key = if keyed { ", requests need the API key" } else { "" };
                eprintln!("Mock OpenAI-compatible server on http://{addr}/v1 ({tps} tokens/s per sequence{key})");
                eprintln!("Mock MCP servers on http://{addr}/mcp and http://{addr}/tracker/mcp");
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
