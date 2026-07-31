//! Proc-macros for SwiftUI-style result-builder widget composition.
//!
//! See `docs/superpowers/specs/2026-07-31-view-builder-design.md`.
//!
//! NOTE: Generated code uses absolute `::vexo::...` paths. Renaming the
//! `vexo` dependency in a downstream crate is unsupported.

use proc_macro::TokenStream;

/// Build a `MultiChild` with `Layout::column()`. See spec § Macro Syntax.
#[proc_macro]
pub fn column(_input: TokenStream) -> TokenStream {
    unimplemented!("column! macro — implemented in Task 3")
}

/// Build a `MultiChild` with `Layout::row()`. See spec § Macro Syntax.
#[proc_macro]
pub fn row(_input: TokenStream) -> TokenStream {
    unimplemented!("row! macro — implemented in Task 3")
}
