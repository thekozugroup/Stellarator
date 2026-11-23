//! Preferences: the list of preference sets (`Route::Preferences`), the review of one
//! (`Route::PreferenceSet`), and their dialogs (new set, auto-pair, export, delete). Ported from
//! frontend/src/pages/preferences-list.tsx, preference-review.tsx and
//! frontend/src/components/preferences/. The pure logic (letters, marks, queues, trace units,
//! labels) is in [`logic`], with its tests.

mod auto_pair;
mod candidate;
mod delete;
mod export;
mod list;
mod logic;
mod new_set;
mod prompt_view;
mod review;

pub use list::screen as list_screen;
pub use review::screen as review_screen;
