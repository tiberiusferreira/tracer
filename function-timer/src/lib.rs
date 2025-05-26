extern crate proc_macro;
use proc_macro::TokenStream;
use quote::quote;
use syn::parse_macro_input;

#[proc_macro_attribute]
pub fn time(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let input = parse_macro_input!(item as syn::ItemFn);
    let attrs = input.attrs;
    let vis = input.vis;
    let func_name = input.sig.ident.clone();
    let sig = input.sig;
    let func_block = &input.block;
    let expanded = quote! {
        #(#attrs)*
        #vis #sig {
            instrument_function_within_task(
                async {
                    #func_block
                },
                stringify!(#func_name)
            )
            .await
        }
    };
    TokenStream::from(expanded)
}
