//! Proc-macros for SwiftUI-style result-builder widget composition.
//!
//! See `docs/superpowers/specs/2026-07-31-view-builder-design.md`.
//!
//! NOTE: Generated code uses absolute `::vexo::...` paths. Renaming the
//! `vexo` dependency in a downstream crate is unsupported.

use proc_macro::TokenStream;
use proc_macro2::TokenTree;
use quote::quote;

/// Entry shared by `column!` and `row!`. `layout_ctor` is either
/// `Layout::column` or `Layout::row`.
fn build_container(input: TokenStream, layout_ctor: &str) -> TokenStream {
    let tokens: proc_macro2::TokenStream = input.into();
    let statements = match split_statements(tokens) {
        Ok(s) => s,
        Err(e) => return e.to_compile_error().into(),
    };

    let layout_path = match layout_ctor {
        "column" => quote! { ::vexo::layout::Layout::column() },
        "row" => quote! { ::vexo::layout::Layout::row() },
        _ => unreachable!(),
    };

    let mut push_calls = Vec::new();
    for stmt in statements {
        match expand_statement(&stmt) {
            Ok(tokens) => push_calls.push(tokens),
            Err(e) => return e.to_compile_error().into(),
        }
    }

    let expanded = quote! {{
        let mut __vexo_children: ::std::vec::Vec<::std::boxed::Box<dyn ::vexo::widgets::Widget>>
            = ::std::vec::Vec::new();
        #(#push_calls)*
        ::vexo::widgets::MultiChild::new(__vexo_children, #layout_path)
    }};
    expanded.into()
}

/// Split a token stream into statements at top-level `,` or `;`.
/// Rejects mixing `,` and `;`.
fn split_statements(
    tokens: proc_macro2::TokenStream,
) -> syn::Result<Vec<proc_macro2::TokenStream>> {
    let mut statements: Vec<proc_macro2::TokenStream> = Vec::new();
    let mut current: Vec<TokenTree> = Vec::new();
    let mut seen_comma = false;
    let mut seen_semi = false;
    let mut first_mixed_span: Option<proc_macro2::Span> = None;

    for tt in tokens {
        match &tt {
            TokenTree::Punct(p) if p.as_char() == ',' || p.as_char() == ';' => {
                if p.as_char() == ',' {
                    seen_comma = true;
                    if seen_semi && first_mixed_span.is_none() {
                        first_mixed_span = Some(p.span());
                    }
                } else {
                    seen_semi = true;
                    if seen_comma && first_mixed_span.is_none() {
                        first_mixed_span = Some(p.span());
                    }
                }
                if !current.is_empty() {
                    statements.push(current.into_iter().collect());
                    current = Vec::new();
                }
            }
            _ => current.push(tt),
        }
    }
    if !current.is_empty() {
        statements.push(current.into_iter().collect());
    }

    if let Some(span) = first_mixed_span {
        return Err(syn::Error::new(
            span,
            "mixing `,` and `;` separators is not allowed inside `column!`/`row!`; pick one",
        ));
    }

    Ok(statements)
}

/// Classify a single statement and emit the corresponding `ChildPush::push_into` call.
///
/// In this task (Task 3), only the "plain widget" case is implemented. The
/// `if` / `for` / `match` / `let` cases are added in Tasks 4-7.
fn expand_statement(stmt: &proc_macro2::TokenStream) -> syn::Result<proc_macro2::TokenStream> {
    // Try to parse as an expression. If it fails, check for `let` (statement,
    // not expression) and emit a clear error; otherwise reparse-error.
    let expr: syn::Expr = match syn::parse2::<syn::Expr>(stmt.clone()) {
        Ok(e) => e,
        Err(_) => {
            // Check for `let` binding.
            let first_token = stmt.clone().into_iter().next();
            if let Some(TokenTree::Ident(ident)) = first_token {
                if ident == "let" {
                    return Err(syn::Error::new(
                        ident.span(),
                        "let bindings are not allowed inside a builder block; compute outside",
                    ));
                }
            }
            return Err(syn::Error::new(
                proc_macro2::Span::call_site(),
                "could not parse statement in builder block",
            ));
        }
    };

    // Plain widget expression.
    Ok(quote! {
        ::vexo::widgets::ChildPush::push_into(#expr, &mut __vexo_children);
    })
}

/// Build a `MultiChild` with `Layout::column()`. See spec § Macro Syntax.
#[proc_macro]
pub fn column(input: TokenStream) -> TokenStream {
    build_container(input, "column")
}

/// Build a `MultiChild` with `Layout::row()`. See spec § Macro Syntax.
#[proc_macro]
pub fn row(input: TokenStream) -> TokenStream {
    build_container(input, "row")
}
