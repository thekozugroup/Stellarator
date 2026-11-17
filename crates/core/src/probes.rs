//! Narrow views of the run engine and the import jobs, so routers in other crates can ask about
//! them without depending on those crates (and their builds).

use futures::future::BoxFuture;

/// A run the engine is driving right now (running or pausing).
#[derive(Clone, Debug)]
pub struct ActiveRun {
    pub run_id: i64,
    pub endpoint_id: Option<i64>,
    pub prompt_set_id: i64,
}

pub trait RunsProbe: Send + Sync {
    fn active_runs(&self) -> Vec<ActiveRun>;
}

pub trait ImportsProbe: Send + Sync {
    /// True while a background import is filling this prompt set.
    fn is_importing(&self, prompt_set_id: i64) -> bool;
    /// Stop a background import (no-op when none runs). Resolves once it has stopped writing.
    fn cancel(&self, prompt_set_id: i64) -> BoxFuture<'_, ()>;
}

/// For tests and partial builds: nothing is ever active.
pub struct Idle;

impl RunsProbe for Idle {
    fn active_runs(&self) -> Vec<ActiveRun> {
        Vec::new()
    }
}

impl ImportsProbe for Idle {
    fn is_importing(&self, _prompt_set_id: i64) -> bool {
        false
    }
    fn cancel(&self, _prompt_set_id: i64) -> BoxFuture<'_, ()> {
        Box::pin(async {})
    }
}
