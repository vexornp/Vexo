use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, DeriveInput, Data, Fields, Type};

#[proc_macro_derive(ComponentState)]
pub fn derive_component_state(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let name = &input.ident;

    let fields = match &input.data {
        Data::Struct(data) => match &data.fields {
            Fields::Named(fields) => &fields.named,
            _ => panic!("#[derive(ComponentState)] only supports structs with named fields"),
        },
        _ => panic!("#[derive(ComponentState)] only supports structs"),
    };

    let mut wire_calls = Vec::new();

    for field in fields {
        let field_name = field.ident.as_ref().unwrap();
        let ty = &field.ty;

        if is_signal_type(ty) || is_option_signal_type(ty) {
            if is_option_signal_type(ty) {
                wire_calls.push(quote! {
                    if let Some(ref mut __field) = self.#field_name {
                        __field.set_dirty_callback(callback.clone());
                    }
                });
            } else {
                wire_calls.push(quote! {
                    self.#field_name.set_dirty_callback(callback.clone());
                });
            }
        }
    }

    let expanded = quote! {
        impl vexo::ComponentState for #name {
            fn set_dirty_callback(&mut self, callback: std::sync::Arc<dyn Fn() + Send + Sync>) {
                #(#wire_calls)*
            }
        }
    };

    TokenStream::from(expanded)
}

fn is_signal_type(ty: &Type) -> bool {
    // Check if type is Signal<T> or vexo::Signal<T> or vexo::reactive::Signal<T>
    let type_str = quote!(#ty).to_string().replace(" ", "");
    type_str.starts_with("Signal<")
        || type_str.starts_with("vexo::Signal<")
        || type_str.starts_with("vexo::reactive::Signal<")
}

fn is_option_signal_type(ty: &Type) -> bool {
    // Check if type is Option<Signal<T>> or qualified variants
    let type_str = quote!(#ty).to_string().replace(" ", "");
    type_str.starts_with("Option<Signal<")
        || type_str.starts_with("Option<vexo::Signal<")
        || type_str.starts_with("Option<vexo::reactive::Signal<")
}
