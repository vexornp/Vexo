//! `Memo` — a `Component` that caches its rendered subtree and only rebuilds
//! when declared dependencies change.
//!
//! This is one of two public APIs for level 2 of the rebuild-skipping ladder
//! (see `docs/rebuild-skipping-patterns.md`); the other is `Shared`. It
//! eliminates the "forgot to cache the `Rc`" footgun that comes with manual
//! `Shared` usage: the user declares *what* the subtree depends on via
//! `deps`, and the framework handles caching internally.
//!
//! ## When to use
//!
//! Use `Memo` when a parent component re-renders frequently (keyboard frames,
//! scroll, animation) but a child subtree only depends on a stable piece of
//! data. Wrap the child build in `Memo::new(deps, || build_subtree(...))` and
//! the framework will skip the child's `render()` + reconciliation cascade
//! whenever `deps` is unchanged across parent rebuilds.
//!
//! ## Contract
//!
//! - `deps: T` must capture **everything** the `build` closure reads that
//!   could change. If `build` reads an `InheritedWidget` value (Theme,
//!   MediaQuery), capture that value in `deps` too — otherwise the cache will
//!   be stale across that dependency's invalidation.
//! - `deps` must implement `PartialEq + Clone`. The comparison is the sole
//!   arbiter of whether to rebuild.
//! - The `build` closure is invoked at most once per unique `deps` value
//!   (specifically: on first mount, and whenever `deps` changes).
//!
//! ## `Memo<()>` is almost never correct
//!
//! Because `()` is always equal, `Memo::new((), …)` blocks **all** parent
//! cascades unconditionally — not just the ones you intended to skip. This is
//! a real footgun: a wrapper component that wraps its (opaque, caller-built)
//! child in `Memo<()>` to skip keyboard-frame rebuilds will *also* skip
//! rebuilds when the parent passes genuinely new content (e.g. switching
//! conversations), leaving stale UI on screen.
//!
//! If the subtree has any input that can change, either:
//! - capture that input in `deps` (requires the wrapper to know what the
//!   child depends on — brittle when the child is opaque), or
//! - use `Shared` instead, which compares the `Rc` pointer of the child
//!   widget itself and thus reconciles whenever the parent builds new content.
//!
//! See `Shared`'s module docs for the wrapper-component pattern.
//!
//! ## What `Memo` does NOT cache
//!
//! `Memo` caches the **widget configuration tree**, not the element or render
//! object trees. Descendants of `Memo` still respond to `Signal::set`,
//! `InheritedWidget` invalidation, and pipeline-driven layout/paint via the
//! state-driven rebuild path — those bypass `should_rebuild()` and re-render
//! the relevant descendant regardless of `Memo`'s cache. This is why
//! `Memo`-wrapped subtrees still update correctly on theme toggles and
//! rotation even when `deps` hasn't changed.
//!
//! ## Example
//!
//! ```ignore
//! // Settings list depends only on `items`; rebuild only when items change.
//! Memo::new(
//!     items.clone(),
//!     || build_settings_list(&items),
//! )
//! ```
//!
//! This is the public API for level 2 of the rebuild-skipping ladder (see
//! `docs/rebuild-skipping-patterns.md`). It eliminates the "forgot to cache
//! the `Rc`" footgun that comes with manual `Shared` usage: the user declares
//! *what* the subtree depends on via `deps`, and the framework handles caching
//! internally.
//!
//! ## When to use
//!
//! Use `Memo` when a parent component re-renders frequently (keyboard frames,
//! scroll, animation) but a child subtree only depends on a stable piece of
//! data. Wrap the child build in `Memo::new(deps, || build_subtree(...))` and
//! the framework will skip the child's `render()` + reconciliation cascade
//! whenever `deps` is unchanged across parent rebuilds.
//!
//! ## Contract
//!
//! - `deps: T` must capture **everything** the `build` closure reads that
//!   could change. If `build` reads an `InheritedWidget` value (Theme,
//!   MediaQuery), capture that value in `deps` too — otherwise the cache will
//!   be stale across that dependency's invalidation.
//! - `deps` must implement `PartialEq + Clone`. The comparison is the sole
//!   arbiter of whether to rebuild.
//! - The `build` closure is invoked at most once per unique `deps` value
//!   (specifically: on first mount, and whenever `deps` changes).
//!
//! ## What `Memo` does NOT cache
//!
//! `Memo` caches the **widget configuration tree**, not the element or render
//! object trees. Descendants of `Memo` still respond to `Signal::set`,
//! `InheritedWidget` invalidation, and pipeline-driven layout/paint via the
//! state-driven rebuild path — those bypass `should_rebuild()` and re-render
//! the relevant descendant regardless of `Memo`'s cache. This is why
//! `Memo`-wrapped subtrees still update correctly on theme toggles and
//! rotation even when `deps` hasn't changed.
//!
//! ## Example
//!
//! ```ignore
//! // Settings list depends only on `items`; rebuild only when items change.
//! Memo::new(
//!     items.clone(),
//!     || build_settings_list(&items),
//! )
//! ```

use std::rc::Rc;

use crate::stateful_widget::{Component, ComponentState, RenderContext};
use crate::widgets::shared::Shared;
use crate::widgets::Widget;

/// A `Component` that caches its rendered subtree, rebuilding only when
/// `deps` changes. See the module docs for the full contract.
pub struct Memo<T: Clone + PartialEq + 'static> {
    deps: T,
    build: Rc<dyn Fn() -> Box<dyn Widget>>,
}

impl<T: Clone + PartialEq + 'static> Memo<T> {
    /// Create a `Memo` that invokes `build` to produce the cached subtree
    /// whenever `deps` changes.
    ///
    /// The closure is `Fn` (not `FnMut` / `FnOnce`): it may be invoked more
    /// than once across the `Memo`'s lifetime (once per unique `deps` value).
    /// Capture only what you need; the closure is stored in an `Rc` so the
    /// `Memo` itself is cheaply cloneable.
    pub fn new(deps: T, build: impl Fn() -> Box<dyn Widget> + 'static) -> Self {
        Self {
            deps,
            build: Rc::new(build),
        }
    }
}

impl<T: Clone + PartialEq + 'static> Clone for Memo<T> {
    fn clone(&self) -> Self {
        Self {
            deps: self.deps.clone(),
            build: Rc::clone(&self.build),
        }
    }
}

/// State for `Memo`: the cached widget subtree (as an `Rc` for O(1) sharing)
/// and the last `deps` value that produced it.
pub struct MemoState<T: Clone + PartialEq + 'static> {
    cached: Option<Rc<dyn Widget>>,
    last_deps: Option<T>,
}

impl<T: Clone + PartialEq + 'static> Default for MemoState<T> {
    fn default() -> Self {
        Self {
            cached: None,
            last_deps: None,
        }
    }
}

impl<T: Clone + PartialEq + 'static> ComponentState for MemoState<T> {}

impl<T: Clone + PartialEq + 'static> Component for Memo<T> {
    type State = MemoState<T>;

    /// Skip `render()` on parent-cascade updates when `deps` is unchanged.
    /// This is the primary optimization: when a parent re-renders for an
    /// unrelated reason (keyboard frame, sibling state change) and passes
    /// down a `Memo` with the same `deps`, the cascade stops here.
    ///
    /// Note: state-driven rebuilds (`rebuild_from_state`, triggered by
    /// `Signal::set` or `InheritedWidget` invalidation on this element)
    /// bypass this hook — but those are rare for `Memo` itself, since `Memo`
    /// holds no `Signal`s and is not an `InheritedWidget` dependent.
    fn should_rebuild(&self, old: &Self) -> bool {
        self.deps != old.deps
    }

    fn render(&self, state: &mut Self::State, _ctx: &mut RenderContext) -> Box<dyn Widget> {
        // Fast path: deps unchanged since last build (can only happen on a
        // state-driven rebuild, since should_rebuild() already gates the
        // parent-cascade path). Reuse the cached subtree.
        if let (Some(last), Some(cached)) = (&state.last_deps, &state.cached) {
            if *last == self.deps {
                return Shared::new(cached.clone()).boxed();
            }
        }

        // Deps changed (or first render): rebuild the subtree, cache it, and
        // wrap in `Shared` so `SharedElement::update()` can skip the child
        // cascade on future parent-cascade updates that don't reach our
        // `render()` (i.e., should_rebuild() returned false).
        let child = (self.build)();
        let rc: Rc<dyn Widget> = Rc::from(child);
        state.cached = Some(rc.clone());
        state.last_deps = Some(self.deps.clone());
        Shared::new(rc).boxed()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::build_owner::BuildOwner;
    use crate::id::ElementKey;
    use crate::inherited_registry::{InheritedMap, InheritedRegistry};
    use crate::widgets::Text;
    use std::sync::{Arc, Mutex};

    fn make_element_key() -> ElementKey {
        let mut sm: slotmap::SlotMap<ElementKey, ()> = slotmap::SlotMap::with_key();
        sm.insert(())
    }

    #[test]
    fn memo_is_clone() {
        let m1 = Memo::new(42u32, || Text::new("hi").boxed());
        let m2 = m1.clone();
        // deps should be equal after clone
        assert_eq!(m1.deps, m2.deps);
    }

    #[test]
    fn memo_state_defaults_empty() {
        let s = MemoState::<u32>::default();
        assert!(s.cached.is_none());
        assert!(s.last_deps.is_none());
    }

    #[test]
    fn memo_should_rebuild_compares_deps() {
        let m1 = Memo::new(1u32, || Text::new("a").boxed());
        let m2 = Memo::new(1u32, || Text::new("b").boxed());
        let m3 = Memo::new(2u32, || Text::new("c").boxed());

        assert!(!m1.should_rebuild(&m2), "same deps → no rebuild");
        assert!(m1.should_rebuild(&m3), "different deps → rebuild");
    }

    #[test]
    fn memo_render_caches_on_first_call_and_reuses_on_same_deps() {
        let call_count = Arc::new(Mutex::new(0u32));
        let cc = call_count.clone();
        let memo = Memo::new((), move || {
            *cc.lock().unwrap() += 1;
            Text::new("hi").boxed()
        });
        let mut state = MemoState::<()>::default();

        let build_owner = BuildOwner::new();
        let map = InheritedMap::empty();
        let reg = InheritedRegistry::new();
        let mut ctx = RenderContext::new(
            make_element_key(),
            &build_owner,
            &map,
            &reg,
            std::sync::Arc::new(|| {}),
        );

        // First render: closure invoked, cache populated.
        let _w1 = memo.render(&mut state, &mut ctx);
        assert_eq!(*call_count.lock().unwrap(), 1);
        assert!(state.cached.is_some());
        assert_eq!(state.last_deps, Some(()));

        // Second render with same deps: closure NOT invoked, cache reused.
        let _w2 = memo.render(&mut state, &mut ctx);
        assert_eq!(*call_count.lock().unwrap(), 1, "closure must not run again");
    }

    #[test]
    fn memo_render_rebuilds_when_deps_change() {
        let call_count = Arc::new(Mutex::new(0u32));
        let cc = call_count.clone();
        let memo = Memo::new(1u32, move || {
            *cc.lock().unwrap() += 1;
            Text::new("hi").boxed()
        });
        let mut state = MemoState::<u32>::default();

        let build_owner = BuildOwner::new();
        let map = InheritedMap::empty();
        let reg = InheritedRegistry::new();
        let mut ctx = RenderContext::new(
            make_element_key(),
            &build_owner,
            &map,
            &reg,
            std::sync::Arc::new(|| {}),
        );

        // First render with deps=1: closure invoked.
        let _w1 = memo.render(&mut state, &mut ctx);
        assert_eq!(*call_count.lock().unwrap(), 1);

        // Simulate deps change by constructing a new Memo with deps=2 and
        // calling render on the same state (as rebuild_from_state would).
        let cc2 = call_count.clone();
        let memo2 = Memo::new(2u32, move || {
            *cc2.lock().unwrap() += 1;
            Text::new("hi").boxed()
        });
        let _w2 = memo2.render(&mut state, &mut ctx);
        assert_eq!(
            *call_count.lock().unwrap(),
            2,
            "closure must run again when deps change"
        );
    }
}
