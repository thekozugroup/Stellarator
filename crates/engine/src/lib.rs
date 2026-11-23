//! The run engine: one chat request at a time ([`generation`]), many at once ([`Engine`]), the
//! tool loop of runs with MCP tools ([`tools`]), and the `/api/runs*` and `/api/samples/{id}`
//! routes ([`router`]).
//!
//! Wiring: `Engine::new(ctx)` once; `engine.recover()` on startup before serving; `router(engine)`
//! merged into the app; `engine.shutdown()` on exit. `Engine` implements `RunsProbe` for the routes
//! that must not delete what a run is using.

pub mod engine;
pub mod generation;
pub mod routes;
pub mod tools;

pub use engine::{Engine, LiveInfo, ENDPOINT_GONE, RESTART_REASON};
pub use routes::router;
