//! Prompt sets: the list (`Route::PromptSets`) and a set's page (`Route::PromptSet`). The list is
//! frontend/src/pages/prompt-sets.tsx, the page prompt-set-detail.tsx, with the pieces of
//! frontend/src/components/prompts/.

mod detail;
mod dialogs;
mod list;
mod row;
mod source;
mod status;

pub use detail::screen as detail;
pub use list::screen as list;
