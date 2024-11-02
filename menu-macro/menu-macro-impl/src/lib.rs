use proc_macro::TokenStream;
use quote::{format_ident, quote};
use syn::{parse_macro_input, Data, DataStruct, DeriveInput, Ident, LitStr};

// TODO: More methods?
// Make cleaner

#[proc_macro_derive(TrayChecks, attributes(menuid))]
pub fn tray_checkboxes_derive(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);

    // requires "extra-traits" feature
    // panic!("{input:#?}");

    let StructInfo {
        menu_id_root,
        struct_name,
    } = parse_struct_info(&input);

    let out = match input.data {
        Data::Struct(s) => {
            // Collect field data
            let named_fields = process_fields(&s, &menu_id_root, true);

            // Generate per-bool checkboxes
            let check_menu_items = named_fields.iter().map(|field_info| {
                let ProcessedField {
                    original_ident,
                    id_method_name: _,
                    doc_string: _,
                    output_menu_id,
                } = field_info;
                quote! {
                    let generated_check_menu_item = muda::CheckMenuItemBuilder::new()
                        .enabled(true)
                        .checked(self.#original_ident)
                        .id(stringify!(#output_menu_id).into())
                        .text(stringify!(#output_menu_id)).build();
                }
            });

            // Generate event-handling method
            let build_checkboxes_doc = "Returns a `Vec<CheckMenuItem>` generated from the struct's bool parameters.\n\nControl generated ids with `#[menuid]` attributes.";
            let build_checkboxes_method = quote! {
                #[doc = #build_checkboxes_doc]
                pub fn build_check_menu_items(&self) -> Vec<muda::CheckMenuItem> {
                    let mut checkboxes = Vec::new();

                    #(
                        #check_menu_items
                        checkboxes.push(generated_check_menu_item);
                    )*

                    checkboxes
                }
            };

            // Generate and return the impl block
            quote! {
                impl #struct_name {
                    #build_checkboxes_method
                }
            }
        }
        Data::Enum(_) | Data::Union(_) => unimplemented!(),
    };

    out.into()
}

#[proc_macro_derive(MenuToggle, attributes(menuid))]
pub fn menu_toggle_derive(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);

    // requires "extra-traits" feature
    // panic!("{input:#?}");

    let StructInfo {
        menu_id_root,
        struct_name,
    } = parse_struct_info(&input);

    let out = match input.data {
        Data::Struct(s) => {
            // Collect field data
            let named_fields = process_fields(&s, &menu_id_root, true);

            // Generate per-field methods
            let matches = named_fields.iter().map(|field_info| {
                let ProcessedField {
                    original_ident,
                    id_method_name: _,
                    doc_string: _,
                    output_menu_id,
                } = field_info;
                quote! {
                    stringify!(#output_menu_id) => {
                        // Flips bool value
                        self.#original_ident ^= true;
                        Ok(())
                    }
                }
            });

            // Generate event-handling method
            let struct_event_handle_doc = "Flips the associated bool of the given menu ID";
            let struct_match_method = quote! {
                #[doc = #struct_event_handle_doc]
                pub fn handle_menu_toggle_event(&mut self, id: &str) -> Result<(), MenuMacroError> {
                    use menu_macro::MenuMacroError;
                    // stringify!(#struct_root)
                    match id {
                        #(#matches)*
                        _ => Err(MenuMacroError::FieldNotFound(id.to_string())),
                    }
                }
            };

            // Generate and return the impl block
            quote! {
                impl #struct_name {
                    #struct_match_method
                }
            }
        }
        Data::Enum(_) | Data::Union(_) => unimplemented!(),
    };

    out.into()
}

#[proc_macro_derive(MenuId, attributes(menuid))]
pub fn menu_id_derive(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    // Shoutout to
    // https://www.reddit.com/r/rust/comments/ke5hct/is_there_a_simple_procmacro_derive_attributes/icx6e9s/
    // for this tip

    // requires "extra-traits" feature
    // panic!("{input:#?}");

    let StructInfo {
        menu_id_root,
        struct_name,
    } = parse_struct_info(&input);

    let out = match input.data {
        Data::Struct(s) => {
            // Collect field data
            let named_fields = process_fields(&s, &menu_id_root, false);

            // Generate struct_root method
            let struct_root_method_name = format_ident!("menu_id_root");
            let struct_root_doc = format!(
                "Returns the root of each of this struct's menu_id methods: `{menu_id_root}`\n\nDefault is the name of the struct."
            );
            let struct_root_method = quote! {
                #[doc = #struct_root_doc]
                pub fn #struct_root_method_name(&self) -> &'static str {
                    stringify!(#menu_id_root)
                }
            };

            // Generate per-field methods
            let methods = named_fields.iter().map(|field_info| {
                let ProcessedField { original_ident: _, id_method_name, doc_string, output_menu_id } = field_info;
                let suffix_method = format_ident!("{id_method_name}_with_suffix");
                let prefix_method = format_ident!("{id_method_name}_with_prefix");
                let suffix_doc = format!("Returns the same as `{id_method_name}` (`{output_menu_id}`), with the supplied string directly appended.");
                let prefix_doc = format!("Returns the same as `{id_method_name}` (`{output_menu_id}`), with the supplied string directly prepended.");
                quote! {
                    #[doc = #doc_string]
                    pub fn #id_method_name(&self) -> &'static str {
                        stringify!(#output_menu_id)
                    }

                    #[doc = #suffix_doc]
                    pub fn #suffix_method<S: AsRef<str>> (&self, suffix: S) -> String {
                        format!("{}{}", self.#id_method_name(), suffix.as_ref())
                    }

                    #[doc = #prefix_doc]
                    pub fn #prefix_method<S: AsRef<str>> (&self, prefix: S) -> String {
                        format!("{}{}", prefix.as_ref(), self.#id_method_name())
                    }
                }
            });

            // Generate and return the impl block
            quote! {
                impl #struct_name {
                    #struct_root_method
                    #(#methods)*
                }
            }
        }
        Data::Enum(_) | Data::Union(_) => unimplemented!(),
    };

    out.into()
}

struct StructInfo {
    struct_name: Ident,
    menu_id_root: Ident,
}

fn parse_struct_info(input: &DeriveInput) -> StructInfo {
    let struct_name = input.ident.clone();
    let mut root_name = input.ident.clone();
    let mut optional_prefix = None;

    for attr in input.attrs.iter() {
        if !attr.path().is_ident("menuid") {
            continue;
        }
        attr.parse_nested_meta(|meta| {
            if meta.path.is_ident("root") {
                let lit: LitStr = meta.value()?.parse()?;
                let new_root: String = lit.value();
                root_name = format_ident!("{new_root}");
                Ok(())
            } else if meta.path.is_ident("prefix") {
                let lit: LitStr = meta.value()?.parse()?;
                let new_root: String = lit.value();
                optional_prefix = Some(format_ident!("{new_root}"));
                Ok(())
            } else {
                panic!("Unknown path on struct top-level");
            }
        })
        .unwrap();
    }

    let menu_id_root = if let Some(prefix) = optional_prefix.as_ref() {
        format_ident!("{prefix}_{root_name}")
    } else {
        root_name
    };

    StructInfo {
        struct_name,
        menu_id_root,
    }
}

struct ProcessedField {
    original_ident: Ident,
    id_method_name: Ident,
    doc_string: String,
    output_menu_id: Ident,
}

fn process_fields(
    input_struct: &DataStruct,
    struct_root: &Ident,
    bools_only: bool,
) -> Vec<ProcessedField> {
    input_struct
        .fields
        .iter()
        .filter_map(|field| {
            // Only handle named fields
            let original_ident = field.ident.as_ref()?;
            let original_ident = original_ident.to_owned();
            let mut field_id = original_ident.clone();
            for attr in &field.attrs {
                let mut skip = false;
                if !attr.path().is_ident("menuid") {
                    continue;
                }

                attr.parse_nested_meta(|meta| {
                    if meta.path.is_ident("rename") {
                        let lit: LitStr = meta.value()?.parse()?;
                        let new_id: String = lit.value();
                        field_id = format_ident!("{new_id}");
                    } else if meta.path.is_ident("skip") {
                        skip = true;
                    } else {
                        panic!("Unknown path on field");
                    }
                    Ok(())
                })
                .unwrap();

                if skip {
                    return None;
                }
            }
            if let syn::Type::Path(ref type_path) = field.ty {
                if bools_only && !type_path.path.is_ident("bool") {
                    panic!("Only bool fields are currently allowed, ignore non-bools with #[menuid(skip)]");
                }
            }
            let id_method_name = format_ident!("{}_menu_id", original_ident);

            let output_menu_id = format_ident!("{struct_root}_{field_id}");

            let doc_string = format!("Returns: `{output_menu_id}`");
            Some(ProcessedField{original_ident, output_menu_id, doc_string, id_method_name})
        })
        .collect()
}
