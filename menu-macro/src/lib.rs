use proc_macro::TokenStream;
use quote::{format_ident, quote};
use syn::{parse_macro_input, Data, DeriveInput, LitStr};

// TODO: More methods?
// Make cleaner

#[proc_macro_derive(MenuId, attributes(menuid))]
pub fn menu_id_derive(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    // Shoutout to
    // https://www.reddit.com/r/rust/comments/ke5hct/is_there_a_simple_procmacro_derive_attributes/icx6e9s/
    // for this tip

    // requires "extra-traits" feature
    // panic!("{:#?}", input);

    let struct_name = input.ident.clone();
    let mut root_name = input.ident;

    for attr in input.attrs {
        if !attr.path().is_ident("menuid") {
            continue;
        }
        attr.parse_nested_meta(|meta| {
            if meta.path.is_ident("root") {
                let lit: LitStr = meta.value()?.parse()?;
                let new_root: String = lit.value();
                root_name = format_ident!("{new_root}");
                return Ok(());
            } else {
                panic!("Unknown path on struct top-level");
            }
        })
        .unwrap();
    }

    let out = match input.data {
        Data::Struct(s) => {
            // Collect field data
            let fields = s
                .fields
                .into_iter()
                .filter_map(|field| {
                    if let Some(ident) = field.ident {
                        let mut base_ident = ident.clone();
                        for attr in &field.attrs {
                            let mut skip = false;
                            if !attr.path().is_ident("menuid") {
                                continue;
                            }

                            attr.parse_nested_meta(|meta| {
                                if meta.path.is_ident("rename") {
                                    let lit: LitStr = meta.value()?.parse()?;
                                    let new_id: String = lit.value();
                                    base_ident = format_ident!("{new_id}");
                                    return Ok(());
                                } else if meta.path.is_ident("skip") {
                                    skip = true;
                                    return Ok(());
                                } else {
                                    panic!("Unknown path on field");
                                }
                            })
                            .unwrap();

                            if skip {
                                return None;
                            }
                        }
                        let method_name = format_ident!("{}_menu_id", ident);
                        let output_id = format_ident!("{root_name}_{base_ident}");
                        let doc_string = format!("Returns: `{output_id}`");
                        Some((method_name, doc_string, output_id))
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>();

            // Generate methods
            let methods = fields.iter().map(|(method_name, doc_string, output_id)| {
                let suffix_method = format_ident!("{method_name}_with_suffix");
                let prefix_method = format_ident!("{method_name}_with_prefix");
                let suffix_doc = format!("Returns the same as `{method_name}` (`{output_id}`), with the supplied string directly appended.");
                let prefix_doc = format!("Returns the same as `{method_name}` (`{output_id}`), with the supplied string directly prepended.");
                quote! {
                    #[doc = #doc_string]
                    pub fn #method_name(&self) -> &'static str {
                        stringify!(#output_id)
                    }

                    #[doc = #suffix_doc]
                    pub fn #suffix_method<S: AsRef<str>> (&self, suffix: S) -> String {
                        format!("{}{}", stringify!(#output_id), suffix.as_ref())
                    }

                    #[doc = #prefix_doc]
                    pub fn #prefix_method<S: AsRef<str>> (&self, prefix: S) -> String {
                        format!("{}{}", prefix.as_ref(), stringify!(#output_id))
                    }
                }
            });

            // Generate and return the impl block
            quote! {
                impl #struct_name {
                    #(#methods)*
                }
            }
        }
        Data::Enum(_) | Data::Union(_) => unimplemented!(),
    };

    out.into()
}
