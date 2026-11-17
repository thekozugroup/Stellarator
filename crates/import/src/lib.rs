//! Prompt imports: files (text, Markdown, CSV, TSV, JSON, JSONL, Parquet), pasted text and Hugging
//! Face datasets, turned into prompt sets in the background.
//!
//! Public API for the server crate:
//! - [`Imports::new`] (`Arc<Imports>`), which implements [`stellarator_core::probes::ImportsProbe`];
//! - [`Imports::recover`] at startup, [`Imports::shutdown`] at exit;
//! - [`router`] for `/api/imports/upload`, `/api/imports/preview` and `/api/imports`.
//!
//! Prompts only: output-looking columns (response, answer, reasoning, chosen, ...) are never picked
//! as the prompt; columns of model generations (response, completion, reasoning, chosen, ...) are
//! left out of meta while reference answers (answer, solution, label) are kept; assistant turns
//! after the chosen user turn are dropped from conversations.

mod api;
pub mod cancel;
pub mod files;
pub mod hf;
mod jobs;
pub mod messages;
pub mod parquet_file;
pub mod problem;
pub mod prompts;
pub mod schema;
pub mod table;
pub mod text;
pub mod upload;

pub use api::router;
pub use jobs::{load_set, Imports, INSERT_BATCH, RESTARTED};
pub use problem::Problem;
