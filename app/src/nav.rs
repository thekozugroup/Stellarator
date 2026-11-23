//! Navigation: one stack of screens per sidebar destination, as the web UI's routes and as
//! macOS and iOS apps keep a stack per tab.
//!
//! - [`Route`] names a screen. Each route belongs to a [`Destination`] (its sidebar row or tab).
//!   A top-level route is the root of its destination's stack; a detail route is pushed on it.
//! - [`open`] switches to a destination and shows the top of its stack; pressing the destination
//!   that is already open goes back to its root, as tab bars do.
//! - [`push`] shows a detail screen over the current one; [`back`] (the toolbar's breadcrumb, the
//!   phone's back button, Cmd+[ or Ctrl+[) returns to the screen under it.
//! - Each entry keeps its scroll position ([`remember_scroll`], [`saved_scroll`]), so going back
//!   lands where the user was. Positions are kept apart from the router, keyed by the entry's id,
//!   so scrolling makes nothing that reads the router run again.
//! - Each entry has a label, what the breadcrumb and the phone's back button call it: the route's
//!   own for a top-level screen, the one given to [`push`] for a detail screen, which the screen can
//!   change once it has loaded ([`set_label`]).
//!
//! The state is one global [`Router`]; [`current`] is reactive, so the shell shows a new screen
//! when it changes.

use std::collections::HashMap;

use nsh_core::prelude::*;

/// The sidebar destinations (and the phone's tabs; Settings is in the phone bar).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Destination {
    #[default]
    Runs,
    PromptSets,
    Endpoints,
    Preferences,
    Settings,
}

impl Destination {
    /// Every destination, in sidebar order.
    pub const ALL: [Destination; 5] =
        [Destination::Runs, Destination::PromptSets, Destination::Endpoints, Destination::Preferences, Destination::Settings];

    /// The destination's root screen.
    pub fn root(self) -> Route {
        match self {
            Destination::Runs => Route::Runs,
            Destination::PromptSets => Route::PromptSets,
            Destination::Endpoints => Route::Endpoints,
            Destination::Preferences => Route::Preferences,
            Destination::Settings => Route::Settings,
        }
    }

    fn index(self) -> usize {
        Destination::ALL.iter().position(|d| *d == self).unwrap_or(0)
    }
}

/// A screen of the app. Add a variant for a new screen, give it its destination and its label
/// below, and its view in `screens::view`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Route {
    // Top-level screens: the roots of the destinations.
    Runs,
    PromptSets,
    Endpoints,
    Preferences,
    Settings,
    // Detail screens, pushed on a destination's stack.
    NewRun,
    Run { id: i64 },
    PromptSet { id: i64 },
    Import,
    PreferenceSet { id: i64 },
    /// Every component on one screen (`STELLARATOR_GALLERY=1`), under Settings.
    Gallery,
}

impl Route {
    /// The destination whose stack the route is on.
    pub fn destination(&self) -> Destination {
        match self {
            Route::Runs | Route::NewRun | Route::Run { .. } => Destination::Runs,
            Route::PromptSets | Route::PromptSet { .. } | Route::Import => Destination::PromptSets,
            Route::Endpoints => Destination::Endpoints,
            Route::Preferences | Route::PreferenceSet { .. } => Destination::Preferences,
            Route::Settings | Route::Gallery => Destination::Settings,
        }
    }

    /// A top-level screen (a destination's root): its title is in the toolbar, it has no
    /// breadcrumb.
    pub fn is_top_level(&self) -> bool {
        self.destination().root() == *self
    }

    /// What the breadcrumb and the back button call the screen before it has loaded anything.
    pub fn label(&self) -> String {
        match self {
            Route::Runs => "Runs".into(),
            Route::PromptSets => "Prompt sets".into(),
            Route::Endpoints => "Endpoints".into(),
            Route::Preferences => "Preferences".into(),
            Route::Settings => "Settings".into(),
            Route::NewRun => "New run".into(),
            Route::Run { id } => format!("Run {id}"),
            Route::PromptSet { id } => format!("Prompt set {id}"),
            Route::Import => "Import prompts".into(),
            Route::PreferenceSet { id } => format!("Preference set {id}"),
            Route::Gallery => "Components".into(),
        }
    }
}

/// One screen in a stack: its route, its label and an id of its own (a screen pushed again is a
/// new entry with a new id).
#[derive(Debug, Clone, PartialEq)]
pub struct Entry {
    pub id: u64,
    pub route: Route,
    pub label: String,
}

/// The stacks and the destination shown. Plain data: the functions below keep it in a global
/// signal; tests drive it directly.
#[derive(Debug, Clone, PartialEq)]
pub struct Router {
    pub destination: Destination,
    stacks: [Vec<Entry>; 5],
    next_id: u64,
}

impl Default for Router {
    fn default() -> Self {
        let mut r = Router { destination: Destination::Runs, stacks: Default::default(), next_id: 0 };
        for d in Destination::ALL {
            let root = r.entry(d.root());
            r.stacks[d.index()].push(root);
        }
        r
    }
}

impl Router {
    fn entry(&mut self, route: Route) -> Entry {
        self.next_id += 1;
        Entry { id: self.next_id, label: route.label(), route }
    }

    fn stack(&self) -> &Vec<Entry> {
        &self.stacks[self.destination.index()]
    }

    fn stack_mut(&mut self) -> &mut Vec<Entry> {
        &mut self.stacks[self.destination.index()]
    }

    /// The entry shown.
    pub fn top(&self) -> &Entry {
        self.stack().last().expect("a stack always has its root")
    }

    /// The entries under the one shown, root first: the breadcrumb's parents.
    pub fn parents(&self) -> &[Entry] {
        let stack = self.stack();
        &stack[..stack.len() - 1]
    }

    /// Shows `destination`: the top of its stack, or its root when it is already shown.
    pub fn open(&mut self, destination: Destination) {
        if self.destination == destination {
            self.stack_mut().truncate(1);
        }
        self.destination = destination;
    }

    /// Shows `route` with `label`: on its destination's stack, over what that stack shows. A
    /// top-level route resets its stack to it. The same route as the top is not pushed twice.
    pub fn push(&mut self, route: Route, label: Option<String>) {
        self.destination = route.destination();
        if route.is_top_level() {
            self.stack_mut().truncate(1);
            return;
        }
        if self.top().route == route {
            if let Some(label) = label {
                self.stack_mut().last_mut().expect("root").label = label;
            }
            return;
        }
        let mut entry = self.entry(route);
        if let Some(label) = label {
            entry.label = label;
        }
        self.stack_mut().push(entry);
    }

    /// Back one screen; false at a root.
    pub fn back(&mut self) -> bool {
        let stack = self.stack_mut();
        if stack.len() > 1 {
            stack.pop();
            true
        } else {
            false
        }
    }

    /// Back to the entry at `depth` (0 is the root), for the breadcrumb's parents.
    pub fn back_to(&mut self, depth: usize) {
        self.stack_mut().truncate(depth + 1);
    }

    /// Replaces the shown screen (after a create: New run becomes the run).
    pub fn replace(&mut self, route: Route, label: Option<String>) {
        if self.stack().len() > 1 && route.destination() == self.destination && !route.is_top_level() {
            self.stack_mut().pop();
        }
        self.push(route, label);
    }

    pub fn set_label(&mut self, label: String) {
        self.stack_mut().last_mut().expect("root").label = label;
    }

}

/// Where each entry was scrolled to, by entry id.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ScrollMemory(HashMap<u64, f32>);

impl ScrollMemory {
    pub fn set(&mut self, entry: u64, y: f32) {
        self.0.insert(entry, y);
    }

    pub fn get(&self, entry: u64) -> f32 {
        self.0.get(&entry).copied().unwrap_or(0.0)
    }
}

/// The app's router.
fn router() -> Signal<Router> {
    global::<Router>()
}

/// The route shown (reactive).
pub fn current() -> Route {
    router().with(|r| r.top().route.clone())
}

/// The destination shown (reactive).
pub fn destination() -> Destination {
    router().with(|r| r.destination)
}

/// The shown screen's label (reactive).
pub fn label() -> String {
    router().with(|r| r.top().label.clone())
}

/// The breadcrumb's parents, root first, with their depths (reactive).
pub fn parents() -> Vec<(usize, String)> {
    router().with(|r| r.parents().iter().enumerate().map(|(i, e)| (i, e.label.clone())).collect())
}

/// Whether there is a screen to go back to (reactive).
pub fn can_go_back() -> bool {
    router().with(|r| !r.parents().is_empty())
}

/// Opens `destination` ([`Router::open`]).
pub fn open(destination: Destination) {
    router().update(|r| r.open(destination));
}

/// Shows `route`; `label` names it in the breadcrumb until the screen sets its own.
pub fn push(route: Route, label: Option<String>) {
    router().update(|r| r.push(route, label));
}

/// Replaces the shown screen with `route`.
pub fn replace(route: Route, label: Option<String>) {
    router().update(|r| r.replace(route, label));
}

/// Back one screen.
pub fn back() -> bool {
    let mut moved = false;
    router().update(|r| moved = r.back());
    moved
}

/// Back to the breadcrumb parent at `depth`.
pub fn back_to(depth: usize) {
    router().update(|r| r.back_to(depth));
}

/// Names the shown screen in the breadcrumb (a run's name once it has loaded).
pub fn set_label(label: impl Into<String>) {
    let label = label.into();
    if untracked(self::label) != label {
        router().update(|r| r.set_label(label));
    }
}

/// The shown entry's id (untracked).
pub fn current_entry() -> u64 {
    router().with_untracked(|r| r.top().id)
}

/// Remembers where entry `entry` is scrolled to. Nothing reads the memory reactively, so writing
/// it makes nothing run again.
pub fn remember_scroll(entry: u64, y: f32) {
    global::<ScrollMemory>().update(|m| m.set(entry, y));
}

/// Where entry `entry` was scrolled to (0 for a new one), read once as its screen is made.
pub fn saved_scroll(entry: u64) -> f32 {
    global::<ScrollMemory>().with_untracked(|m| m.get(entry))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_destination_keeps_its_own_stack() {
        let mut r = Router::default();
        assert_eq!(r.top().route, Route::Runs);
        r.push(Route::Run { id: 4 }, Some("Distill gsm8k".into()));
        assert_eq!(r.top().route, Route::Run { id: 4 });
        assert_eq!(r.parents().len(), 1);
        r.open(Destination::Endpoints);
        assert_eq!(r.top().route, Route::Endpoints);
        // Back to Runs: the run is still open.
        r.open(Destination::Runs);
        assert_eq!(r.top().label, "Distill gsm8k");
        // Runs again: its root.
        r.open(Destination::Runs);
        assert_eq!(r.top().route, Route::Runs);
    }

    #[test]
    fn pushing_switches_to_the_routes_destination() {
        let mut r = Router::default();
        r.push(Route::PromptSet { id: 2 }, None);
        assert_eq!(r.destination, Destination::PromptSets);
        assert_eq!(r.top().label, "Prompt set 2");
        assert_eq!(r.parents()[0].route, Route::PromptSets);
        // The same route twice stays one entry.
        r.push(Route::PromptSet { id: 2 }, Some("gsm8k".into()));
        assert_eq!(r.parents().len(), 1);
        assert_eq!(r.top().label, "gsm8k");
    }

    #[test]
    fn back_keeps_the_parents_scroll_position() {
        let mut r = Router::default();
        let mut memory = ScrollMemory::default();
        memory.set(r.top().id, 640.0);
        r.push(Route::Run { id: 1 }, None);
        assert_eq!(memory.get(r.top().id), 0.0);
        let run = r.top().id;
        memory.set(run, 120.0);
        assert!(r.back());
        assert_eq!(r.top().route, Route::Runs);
        assert_eq!(memory.get(r.top().id), 640.0);
        assert!(!r.back());
        // The same run opened again is a new entry: it starts at the top.
        r.push(Route::Run { id: 1 }, None);
        assert_ne!(r.top().id, run);
        assert_eq!(memory.get(r.top().id), 0.0);
    }

    #[test]
    fn replace_and_back_to() {
        let mut r = Router::default();
        r.push(Route::NewRun, None);
        r.replace(Route::Run { id: 9 }, Some("New one".into()));
        assert_eq!(r.parents().len(), 1);
        assert_eq!(r.top().route, Route::Run { id: 9 });
        r.push(Route::Run { id: 10 }, None);
        r.back_to(0);
        assert_eq!(r.top().route, Route::Runs);
        // A top-level route resets its stack.
        r.push(Route::Run { id: 3 }, None);
        r.push(Route::Runs, None);
        assert!(r.parents().is_empty());
    }
}
