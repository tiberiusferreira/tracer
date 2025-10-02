use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, DeriveInput};

#[proc_macro_derive(ToParameters)]
pub fn derive_to_parameters(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);

    let name = input.ident.clone();

    let fields = match input.data {
        syn::Data::Struct(s) => s.fields,
        _ => {
            return syn::Error::new_spanned(input, "ToParameters can only be derived for structs")
                .to_compile_error()
                .into();
        }
    };

    let inserts = fields.iter().map(|f| {
        let field_name = f.ident.as_ref().unwrap();
        let field_str = field_name.to_string();
        quote! {
            map.insert(#field_str.to_string(), ::core::convert::From::from(self.#field_name.clone()));
        }
    });

    let expanded = quote! {
        impl gel_io_recorder::ToParameters for #name {
            fn to_parameters(&self) -> ::indexmap::IndexMap<String, gel_io_recorder::Parameter> {
                let mut map = ::indexmap::IndexMap::new();
                #(#inserts)*
                map
            }
        }
    };

    TokenStream::from(expanded)
}
