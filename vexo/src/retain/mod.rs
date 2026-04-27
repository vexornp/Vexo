//! Retain-mode rendering system (Widget/Element/RenderObject trees).
//!
//! This module implements Flutter-style three-tree architecture for
//! efficient incremental updates.

mod key;
mod id;
mod state;
mod element;
mod element_context;
mod render_object;
mod dirty;
mod reconcile;
mod widgets;

#[cfg(test)]
mod key_tests;
#[cfg(test)]
mod reconcile_tests;
#[cfg(test)]
mod element_registry_tests;

pub use key::Key;
pub use id::{ElementId, RenderObjectId};
pub use state::StateStorage;
pub use element::{Element, ElementRegistry};
pub use element_context::ElementContext;
pub use render_object::{RenderObject, RenderObjectRegistry};
pub use dirty::DirtyTracking;
pub use reconcile::Reconcilable;
pub use widgets::Widget;