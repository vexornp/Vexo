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
/// Handles `if` (with and without `else`) by routing through the `view_builder`
/// helpers. Plain widget expressions go straight through `ChildPush::push_into`.
/// `for` / `match` / `let` are added in later tasks.
fn expand_statement(stmt: &proc_macro2::TokenStream) -> syn::Result<proc_macro2::TokenStream> {
    let expr: syn::Expr = match syn::parse2::<syn::Expr>(stmt.clone()) {
        Ok(e) => e,
        Err(_) => {
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

    match &expr {
        syn::Expr::If(if_expr) => {
            let cond = &if_expr.cond;
            let then_body = &if_expr.then_branch;
            if let Some((_, else_body)) = &if_expr.else_branch {
                // if cond { a } else { b } -> build_either(if c { a.boxed() } else { b.boxed() })
                //
                // `Widget::boxed` is fully-qualified so the macro is self-contained
                // (works regardless of what's in scope at the call site), matching the
                // spec's "Absolute paths" invariant. A `let` binding evaluates the
                // block before boxing: this avoids `unused_braces` warnings that would
                // arise from placing the block directly in function-argument position.
                Ok(quote! {
                    ::vexo::widgets::ChildPush::push_into(
                        ::vexo::view_builder::build_either(
                            if #cond {
                                let __vexo_w = #then_body;
                                ::vexo::widgets::Widget::boxed(__vexo_w)
                            } else {
                                let __vexo_w = #else_body;
                                ::vexo::widgets::Widget::boxed(__vexo_w)
                            }
                        ),
                        &mut __vexo_children,
                    );
                })
            } else {
                // if cond { body } (no else) -> build_optional(if c { Some(body.boxed()) } else { None })
                Ok(quote! {
                    ::vexo::widgets::ChildPush::push_into(
                        ::vexo::view_builder::build_optional(
                            if #cond {
                                let __vexo_w = #then_body;
                                ::std::option::Option::Some(::vexo::widgets::Widget::boxed(__vexo_w))
                            } else {
                                ::std::option::Option::None
                            }
                        ),
                        &mut __vexo_children,
                    );
                })
            }
        }
        _ => {
            // Plain widget expression.
            Ok(quote! {
                ::vexo::widgets::ChildPush::push_into(#expr, &mut __vexo_children);
            })
        }
    }
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
