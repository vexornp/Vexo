//! Retain-mode rendering system (Widget/Element/RenderObject trees).
//!
//! This module implements Flutter-style three-tree architecture for
//! efficient incremental updates.
//!
//! # Architecture
//!
//! The three trees work together:
//!
//! - **Widget tree**: Immutable configuration, rebuilt each frame
//! - **Element tree**: Stateful lifecycle, persistent across frames
//! - **RenderObject tree**: Layout and painting, dirty tracking
//!
//! # Example
//!
//! ```ignore
//! use vexo::retain::{Column, Text, ThreeTreePipeline, Widget};
//!
//! let mut pipeline = ThreeTreePipeline::new();
//!
//! // Create widget tree
//! let widget = Column::new()
//!     .push(Text::new("Hello"))
//!     .push(Text::new("World"));
//!
//! // Reconcile with element tree
//! pipeline.reconcile(Box::new(widget));
//!
//! // Layout and paint
//! pipeline.layout(available_size, &mut layout_engine);
//! let commands = pipeline.paint();
//! ```
//!
//! # Migration from Immediate Mode
//!
//! The retain-mode system can coexist with the immediate-mode system.
//! Set `use_retain_mode = true` in WindowState to enable.

mod key;
mod id;
mod state;
mod element;
mod element_context;
mod event_context;
mod render_object;
mod dirty;
mod build_owner;
mod reconcile;
mod render_objects;
mod hit_test;
mod pipeline;
mod global_key_registry;
mod style;
mod update_result;
mod stateful_widget;

pub mod widgets;
pub mod elements;

#[cfg(test)]
mod key_tests;
#[cfg(test)]
mod reconcile_tests;
#[cfg(test)]
mod element_registry_tests;
#[cfg(test)]
mod integration_tests;
#[cfg(test)]
mod e2e_test;
#[cfg(test)]
mod window_integration_test;
#[cfg(test)]
mod build_owner_tests;
#[cfg(test)]
mod stateful_integration_test;

pub use key::{Key, GlobalKey, WidgetKey};
pub use id::{ElementId, RenderObjectId};
pub use state::StateStorage;
pub use element::{Element, ElementRegistry};
pub use element_context::ElementContext;
pub use event_context::EventContext;
pub use render_object::{RenderObject, RenderObjectRegistry, LayoutContext, LayoutResult, PaintContext, HitTestContext};
pub use dirty::DirtyTracking;
pub use build_owner::{BuildOwner, RebuildResult};
pub use reconcile::Reconcilable;
pub use hit_test::HitTestResult;
pub use global_key_registry::{GlobalKeyRegistry, GlobalKeyError};
pub use style::Style;
pub use update_result::UpdateResult;
pub use stateful_widget::{StatefulWidget, BuildContext, StatefulElement, EmptyRenderObject};

pub use widgets::{Widget, Text, Button, Column, Row, DecoratedContainer};
pub use elements::{LeafElement, ContainerElement};
pub use render_objects::{TextRenderObject, ContainerRenderObject};
pub use pipeline::ThreeTreePipeline;