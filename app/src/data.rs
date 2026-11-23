//! Loading data from the API into a screen: what a list or a detail has ([`Load`]) and the call
//! that fills it ([`call`]). The web UI's react-query, reduced to what the screens use: a screen
//! keeps a `Signal<Load<T>>`, reads it in its views, and calls [`call`] again to refresh it (after a
//! change, or on a timer).
//!
//! ```ignore
//! let endpoints = signal(Load::Loading);
//! let reload = move || call(|api| async move { api.endpoints().await }, move |r| endpoints.set(Load::from(r)));
//! reload();
//! ```

use std::cell::Cell;
use std::future::Future;

use nsh_core::prelude::*;

use crate::api::{Api, ApiError};

/// What a screen has of something it loads.
#[derive(Debug, Clone, PartialEq)]
pub enum Load<T> {
    /// The first answer has not come yet (show placeholder bars at the real size).
    Loading,
    Loaded(T),
    /// The call failed: the sentence to show in an error block, with Retry.
    Failed(String),
}

impl<T> Load<T> {
    pub fn is_loading(&self) -> bool {
        matches!(self, Load::Loading)
    }

    pub fn loaded(&self) -> Option<&T> {
        match self {
            Load::Loaded(v) => Some(v),
            _ => None,
        }
    }

    pub fn error(&self) -> Option<&str> {
        match self {
            Load::Failed(e) => Some(e),
            _ => None,
        }
    }
}

impl<T> From<Result<T, ApiError>> for Load<T> {
    fn from(result: Result<T, ApiError>) -> Self {
        match result {
            Ok(v) => Load::Loaded(v),
            Err(e) => Load::Failed(e.message),
        }
    }
}

/// The owner of the open screen, provided as a context by `screens::view`.
#[derive(Clone, Copy)]
struct ScreenOwner(Owner);

thread_local! {
    /// The open screen's owner, for code whose own owner was disposed while it ran (see
    /// [`screen_owner`]). One screen is open at a time; it clears itself when it goes.
    static OPEN_SCREEN: Cell<Option<Owner>> = const { Cell::new(None) };
}

/// Makes the calling scope (a screen being made) the owner of the API calls made below it. A call
/// made from a menu item or a dialog then outlives the popup that started it: closing the menu
/// does not cancel the request, leaving the screen does.
pub fn own_calls() {
    let owner = Owner::current();
    provide_context(ScreenOwner(owner));
    OPEN_SCREEN.with(|s| s.set(Some(owner)));
    cleanup(move || {
        OPEN_SCREEN.with(|s| {
            if s.get() == Some(owner) {
                s.set(None);
            }
        })
    });
}

/// The owner a request, a timer or a task started now should belong to: the open screen's.
///
/// nsh runs a press handler under the view's owner and does not batch it: a signal written in the
/// handler runs its bindings before the handler goes on. When the handler writes a signal that
/// the `dynamic` (or `when`) holding the pressed control reads, the region is made again at once
/// and the handler's owner is disposed under it (nsh-core `reactive.rs`, `dispose`: the current
/// owner becomes `DISPOSED`). From then on the handler finds no context, and a task it spawns is
/// cancelled at once. So this looks for the screen in three places, in order:
///
/// 1. the screen the running code belongs to (its context), while its owner lives;
/// 2. the open screen ([`own_calls`] keeps it), when the running code's owner was disposed;
/// 3. outside any screen, the current owner.
pub fn screen_owner() -> Owner {
    if let Some(ScreenOwner(owner)) = use_context::<ScreenOwner>()
        && owner.is_live()
    {
        return owner;
    }
    if let Some(owner) = OPEN_SCREEN.with(Cell::get)
        && owner.is_live()
    {
        return owner;
    }
    Owner::current()
}

/// Runs `request` against the API in a task of the open screen ([`screen_owner`]), then `done`
/// with its result, as one batch ([`finish`]). Where the API cannot start (the in-process server
/// failed), `done` gets that error.
///
/// The task ends with the screen, not with the control, menu or dialog that started it. So `done`
/// may run after the region it was written in has gone (a dialog closed, a `dynamic` made again):
/// the signals it writes must outlive the request. Keep them at the screen's top level (or beside
/// a dialog, not in its content), or write them with `try_set` / [`SignalExt::try_update`].
pub fn call<T, F, Fut>(request: F, done: impl FnOnce(Result<T, ApiError>) + 'static)
where
    T: 'static,
    F: FnOnce(Api) -> Fut + 'static,
    Fut: Future<Output = Result<T, ApiError>> + 'static,
{
    screen_owner().run(move || {
        let _ = spawn(async move {
            let result = match crate::api() {
                Ok(api) => request(api).await,
                Err(message) => Err(ApiError { status: None, message, fields: Default::default() }),
            };
            finish(done, result);
        });
    });
}

/// Runs `done` with `result` as one batch: its writes apply together at its end. A write that
/// makes a region again (the screen's `load`) then does not dispose, halfway through `done`, the
/// signals of that region `done` writes next (an `editing` flag in it).
fn finish<T>(done: impl FnOnce(T), result: T) {
    batch(move || done(result));
}

/// What nsh 0.6.1's `Signal` lacks next to `try_set`.
pub trait SignalExt<T> {
    /// Changes the value in place and writes it; false (and nothing written) once the signal's
    /// owner was disposed. Reading it subscribes nothing.
    fn try_update(&self, f: impl FnOnce(&mut T)) -> bool;
}

impl<T: Clone + 'static> SignalExt<T> for Signal<T> {
    fn try_update(&self, f: impl FnOnce(&mut T)) -> bool {
        let Some(mut value) = untracked(|| self.try_get()) else { return false };
        f(&mut value);
        self.try_set(value)
    }
}

/// The current time, from nsh's clock (the same on native targets and the web).
pub fn now_utc() -> chrono::DateTime<chrono::Utc> {
    chrono::DateTime::from_timestamp_millis(nsh_core::time::unix_ms() as i64).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use std::rc::Rc;

    use nsh_core::element::{Runtime, create_root};

    use super::*;

    /// What a region made last: its owner, and a signal made in it (a dialog's or a form's state).
    type Made = Rc<Cell<Option<(Owner, Signal<bool>)>>>;

    /// A region as `dynamic` makes it: a binding that reads `flag` and, each time it runs, makes a
    /// new owner for what it shows and disposes the one before.
    fn region(flag: Signal<u32>, made: Made) {
        effect(move || {
            flag.get();
            let (owner, field) = create_root(|| signal(false));
            cleanup(move || owner.dispose());
            made.set(Some((owner, field)));
        });
    }

    /// A screen as `screens::view` makes it: a root that owns its calls ([`own_calls`]), with a
    /// region in it.
    fn screen() -> (Owner, Signal<u32>, Made) {
        let made: Made = Rc::new(Cell::new(None));
        let (owner, flag) = create_root({
            let made = made.clone();
            move || {
                own_calls();
                let flag = signal(0_u32);
                region(flag, made);
                flag
            }
        });
        (owner, flag, made)
    }

    #[test]
    fn a_press_that_rebuilds_its_own_region_still_calls_under_the_screen() {
        let runtime = Runtime::default();
        let _entered = runtime.enter();
        let (screen, flag, made) = screen();
        let (pressed, _) = made.get().expect("the region was made");

        // The handler writes what its region reads: unbatched, the region is made again at once.
        flag.set(1);
        assert!(!pressed.is_live(), "the region was made again and its old owner disposed");

        // The rest of the handler runs under that disposed owner (nsh-core `dispose` makes the
        // current owner DISPOSED). `Owner::run` batches, so the test disposes the owner it runs
        // under where the write did.
        let (pressed, _) = made.get().unwrap();
        pressed.run(move || {
            pressed.dispose();
            assert!(!Owner::current().is_live());
            assert!(use_context::<ScreenOwner>().is_none(), "no context under a disposed owner");

            // Before the fix the call's task belonged to the current owner: cancelled at once.
            let cancelled = Rc::new(Cell::new(false));
            let c = cancelled.clone();
            Owner::current().run(move || cleanup(move || c.set(true)));
            assert!(cancelled.get(), "a task spawned under a disposed owner ends at once");

            // Now the open screen owns it, and it runs until the screen goes.
            let owner = screen_owner();
            assert_eq!(owner, screen);
            let ended = Rc::new(Cell::new(false));
            let e = ended.clone();
            owner.run(move || cleanup(move || e.set(true)));
            assert!(!ended.get(), "the call's task outlives the pressed control");
        });
        screen.dispose();
        assert_eq!(OPEN_SCREEN.with(Cell::get), None, "a screen that goes clears itself");
    }

    #[test]
    fn a_live_owner_finds_its_screen_through_the_context() {
        let runtime = Runtime::default();
        let _entered = runtime.enter();
        let (screen, _, made) = screen();
        let (region_owner, _) = made.get().unwrap();
        assert_eq!(region_owner.run(screen_owner), screen);
        // Outside any screen: the current owner.
        screen.dispose();
        let (other, found) = create_root(screen_owner);
        assert_eq!(found, other);
    }

    #[test]
    fn done_runs_as_one_batch() {
        let runtime = Runtime::default();
        let _entered = runtime.enter();
        let (_screen, flag, made) = screen();
        let (_, editing) = made.get().unwrap();

        // Unbatched, writing what the region reads disposes the region's signals at once: `set`
        // would panic ("written after its owner was disposed"); `try_set` says so.
        flag.set(1);
        assert!(!editing.try_set(true));

        // `call` runs `done` as one batch: the region is made again once `done` ends, so a signal
        // of the region that `done` writes after the screen's is still live.
        let (_, editing) = made.get().unwrap();
        let wrote = Rc::new(Cell::new(false));
        let w = wrote.clone();
        finish(
            move |n: u32| {
                flag.set(n);
                editing.set(true);
                w.set(true);
            },
            2,
        );
        assert!(wrote.get());
        let (_, rebuilt) = made.get().unwrap();
        assert!(rebuilt != editing, "the region was made again once `done` ended");
    }

    #[test]
    fn try_update_changes_a_live_signal_and_skips_a_disposed_one() {
        let runtime = Runtime::default();
        let _entered = runtime.enter();
        let (owner, list) = create_root(|| signal(vec![1]));
        assert!(list.try_update(|l| l.push(2)));
        assert_eq!(list.get_untracked(), vec![1, 2]);
        owner.dispose();
        assert!(!list.try_update(|l| l.push(3)));
    }
}
