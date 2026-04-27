//! Element and RenderObject identifiers.

/// Unique identifier for an Element in the Element tree.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ElementId(usize);

/// Unique identifier for a RenderObject in the RenderObject tree.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct RenderObjectId(usize);
