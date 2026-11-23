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
//! - [`reset_to`] shows a screen as a fresh page of its destination (`[root, route]`), where the
//!   web UI navigates to a new URL: a run once it starts, a prompt set once its import starts.
//! - Each route has the web UI's path ([`Route::path`], [`Route::from_path`]); the web build keeps
//!   it in the address bar and follows the browser's back and forward ([`show`]).
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

    // The web build follows the address bar with it; the desktop has none.
    #[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
    /// The web UI's path of the route (frontend/src/App.tsx): `/runs`, `/runs/new`, `/runs/4`.
    pub fn path(&self) -> String {
        match self {
            Route::Runs => "/runs".into(),
            Route::NewRun => "/runs/new".into(),
            Route::Run { id } => format!("/runs/{id}"),
            Route::PromptSets => "/prompts".into(),
            Route::Import => "/prompts/import".into(),
            Route::PromptSet { id } => format!("/prompts/{id}"),
            Route::Endpoints => "/endpoints".into(),
            Route::Preferences => "/preferences".into(),
            Route::PreferenceSet { id } => format!("/preferences/{id}"),
            Route::Settings => "/settings".into(),
            Route::Gallery => "/settings/components".into(),
        }
    }

    /// The route of a path or of a whole address (`http://host/runs/4?sample=2#x`): its query and
    /// fragment are left out, as is a trailing slash. `/` is Runs, as the web UI redirects it;
    /// `None` for a path the web UI has no page for.
    // The web build follows the address bar with it; the desktop has none.
    #[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
    pub fn from_path(address: &str) -> Option<Route> {
        let parts: Vec<&str> = path_of_address(address).split('/').filter(|p| !p.is_empty()).collect();
        let id = |s: &str| s.parse::<i64>().ok().filter(|n| *n > 0);
        Some(match parts.as_slice() {
            [] => Route::Runs,
            ["runs"] => Route::Runs,
            ["runs", "new"] => Route::NewRun,
            ["runs", n] => Route::Run { id: id(n)? },
            ["prompts"] => Route::PromptSets,
            ["prompts", "import"] => Route::Import,
            ["prompts", n] => Route::PromptSet { id: id(n)? },
            ["endpoints"] => Route::Endpoints,
            ["preferences"] => Route::Preferences,
            ["preferences", n] => Route::PreferenceSet { id: id(n)? },
            ["settings"] => Route::Settings,
            ["settings", "components"] => Route::Gallery,
            _ => return None,
        })
    }
}

/// The path of `address` (`/runs/4` of `http://host/runs/4?sample=2#x`): a bare path as it is,
/// less its query and fragment; `/` for an address with no path.
#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
pub fn path_of_address(address: &str) -> &str {
    let path = match address.split_once("://") {
        Some((_, rest)) => rest.find('/').map(|i| &rest[i..]).unwrap_or("/"),
        None => address,
    };
    match path.split(['?', '#']).next() {
        Some("") | None => "/",
        Some(p) => p,
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

    /// Shows `route` as a fresh page of its destination: its stack becomes the root and the
    /// route (the root alone for a top-level route). Back then goes to the destination's root, as
    /// the web UI's breadcrumb does after it navigates to a new page.
    pub fn reset_to(&mut self, route: Route, label: Option<String>) {
        self.destination = route.destination();
        self.stack_mut().truncate(1);
        self.push(route, label);
    }

    /// Shows `route` where the browser's back or forward (or a typed address) went: the entry
    /// already on its destination's stack when there is one (keeping the entries under it and
    /// their scroll positions), else as a fresh page ([`Router::reset_to`]).
    // The web build follows the address bar with it; the desktop has none.
    #[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
    pub fn show(&mut self, route: Route) {
        self.destination = route.destination();
        match self.stack().iter().position(|e| e.route == route) {
            Some(i) => self.stack_mut().truncate(i + 1),
            None => self.reset_to(route, None),
        }
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

/// Shows `route` as a fresh page of its destination ([`Router::reset_to`]).
pub fn reset_to(route: Route, label: Option<String>) {
    router().update(|r| r.reset_to(route, label));
}

/// Shows `route` where the browser moved to ([`Router::show`]).
#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
pub fn show(route: Route) {
    router().update(|r| r.show(route));
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
    fn reset_to_and_back_to() {
        let mut r = Router::default();
        r.push(Route::NewRun, None);
        r.reset_to(Route::Run { id: 9 }, Some("New one".into()));
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

    #[test]
    fn reset_to_leaves_the_root_and_the_route() {
        let mut r = Router::default();
        // Run again: Runs, the run, New run; starting it leaves Runs and the new run.
        r.push(Route::Run { id: 4 }, Some("Distill".into()));
        r.push(Route::NewRun, None);
        r.reset_to(Route::Run { id: 5 }, Some("Distill again".into()));
        assert_eq!(r.destination, Destination::Runs);
        assert_eq!(r.parents().iter().map(|e| e.route.clone()).collect::<Vec<_>>(), vec![Route::Runs]);
        assert_eq!(r.top().label, "Distill again");
        assert!(r.back());
        assert_eq!(r.top().route, Route::Runs);
        // From another destination: an import started from Prompt sets lands on its set.
        r.push(Route::Import, None);
        r.reset_to(Route::PromptSet { id: 7 }, None);
        assert_eq!(r.destination, Destination::PromptSets);
        assert_eq!(r.parents().len(), 1);
        assert_eq!(r.top().route, Route::PromptSet { id: 7 });
        // A top-level route is its root alone.
        r.reset_to(Route::PromptSets, None);
        assert!(r.parents().is_empty());
    }

    #[test]
    fn show_goes_back_to_an_entry_on_the_stack_or_opens_a_fresh_page() {
        let mut r = Router::default();
        r.push(Route::Run { id: 4 }, Some("Distill".into()));
        let run = r.top().id;
        r.push(Route::NewRun, None);
        r.open(Destination::Endpoints);
        // The browser's back to /runs/4: the run's entry, with its label and id (its scroll).
        r.show(Route::Run { id: 4 });
        assert_eq!(r.destination, Destination::Runs);
        assert_eq!(r.top().id, run);
        assert_eq!(r.top().label, "Distill");
        assert_eq!(r.parents().len(), 1);
        // A route not on the stack (a typed address): Runs, then it.
        r.show(Route::Run { id: 9 });
        assert_eq!(r.parents().len(), 1);
        assert_eq!(r.top().route, Route::Run { id: 9 });
        r.show(Route::Runs);
        assert!(r.parents().is_empty());
        r.show(Route::PreferenceSet { id: 2 });
        assert_eq!(r.destination, Destination::Preferences);
        assert_eq!(r.parents()[0].route, Route::Preferences);
    }

    #[test]
    fn paths_are_the_web_uis() {
        let all = [
            (Route::Runs, "/runs"),
            (Route::NewRun, "/runs/new"),
            (Route::Run { id: 4 }, "/runs/4"),
            (Route::PromptSets, "/prompts"),
            (Route::Import, "/prompts/import"),
            (Route::PromptSet { id: 12 }, "/prompts/12"),
            (Route::Endpoints, "/endpoints"),
            (Route::Preferences, "/preferences"),
            (Route::PreferenceSet { id: 3 }, "/preferences/3"),
            (Route::Settings, "/settings"),
            (Route::Gallery, "/settings/components"),
        ];
        for (route, path) in all {
            assert_eq!(route.path(), path);
            assert_eq!(Route::from_path(path), Some(route));
        }
    }

    #[test]
    fn addresses_map_to_routes() {
        assert_eq!(Route::from_path("/"), Some(Route::Runs));
        assert_eq!(Route::from_path(""), Some(Route::Runs));
        assert_eq!(Route::from_path("http://127.0.0.1:8000"), Some(Route::Runs));
        assert_eq!(Route::from_path("http://127.0.0.1:8000/"), Some(Route::Runs));
        assert_eq!(Route::from_path("http://127.0.0.1:8000/runs/4?sample=11#top"), Some(Route::Run { id: 4 }));
        assert_eq!(Route::from_path("https://host/prompts/import/"), Some(Route::Import));
        assert_eq!(Route::from_path("/runs/new?from=4"), Some(Route::NewRun));
        // Pages the web UI does not have.
        assert_eq!(Route::from_path("/runs/abc"), None);
        assert_eq!(Route::from_path("/runs/0"), None);
        assert_eq!(Route::from_path("/runs/4/samples"), None);
        assert_eq!(Route::from_path("/nope"), None);
    }

    #[test]
    fn the_path_of_an_address() {
        assert_eq!(path_of_address("http://127.0.0.1:8000/runs/4?sample=11#a"), "/runs/4");
        assert_eq!(path_of_address("http://127.0.0.1:8000"), "/");
        assert_eq!(path_of_address("http://127.0.0.1:8000/"), "/");
        assert_eq!(path_of_address("/prompts/import"), "/prompts/import");
        assert_eq!(path_of_address(""), "/");
    }
}
