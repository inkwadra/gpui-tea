use crate::spec::{ChildSpec, CompositeSpec};
use proc_macro2::Span;
use quote::ToTokens;
use std::collections::{HashMap, HashSet};
use syn::{
    Attribute, Data, DeriveInput, Error, Expr, Field, Fields, FieldsNamed, Ident, Lit, LitStr,
    Meta, Result, Token, Type, punctuated::Punctuated, spanned::Spanned,
};

/// # Errors
///
/// Returns an error if the input does not define a valid struct with named fields
/// or is missing the `#[composite(message = ...)]` attribute.
pub(crate) fn parse_composite(input: DeriveInput) -> Result<CompositeSpec> {
    let DeriveInput {
        attrs,
        ident,
        generics,
        data,
        ..
    } = input;

    let message_ty = parse_message_type(&attrs, composite_derive_span(&attrs))?;
    let children = parse_children(named_fields(data, &ident)?)?;

    Ok(CompositeSpec {
        struct_ident: ident,
        generics,
        message_ty,
        children,
    })
}

fn named_fields(data: Data, struct_ident: &Ident) -> Result<FieldsNamed> {
    match data {
        Data::Struct(data) => match data.fields {
            Fields::Named(fields) => Ok(fields),
            _ => Err(Error::new(
                data.struct_token.span(),
                "`Composite` supports only structs with named fields",
            )),
        },
        _ => Err(Error::new(
            struct_ident.span(),
            "`Composite` supports only structs with named fields",
        )),
    }
}

fn composite_derive_span(attrs: &[Attribute]) -> Span {
    for attr in attrs {
        if !attr.path().is_ident("derive") {
            continue;
        }

        let mut derive_span = None;
        if attr
            .parse_nested_meta(|meta| {
                if meta.path.is_ident("Composite") {
                    derive_span = Some(meta.path.span());
                }
                Ok(())
            })
            .is_ok()
            && let Some(span) = derive_span
        {
            return span;
        }
    }

    Span::call_site()
}

/// # Errors
///
/// Returns an error if the `#[composite(message = ...)]` attribute is missing or malformed.
fn parse_message_type(attrs: &[Attribute], missing_message_span: Span) -> Result<Type> {
    let mut message_ty = None;
    let mut errors = None;

    for attr in attrs {
        if !attr.path().is_ident("composite") {
            continue;
        }

        let metas = attr.parse_args_with(Punctuated::<Meta, Token![,]>::parse_terminated)?;
        for meta in metas {
            parse_composite_meta(meta, &mut message_ty, &mut errors);
        }
    }

    if message_ty.is_none() {
        combine_error(
            &mut errors,
            Error::new(missing_message_span, "missing `message`"),
        );
    }

    match (message_ty, errors) {
        (_, Some(error)) => Err(error),
        (Some(message_ty), None) => Ok(message_ty),
        (None, None) => Err(Error::new(missing_message_span, "missing `message`")),
    }
}

/// # Errors
///
/// Returns an error if any child field has invalid attributes or if multiple children
/// generate identically named helper methods.
fn parse_children(fields: FieldsNamed) -> Result<Vec<ChildSpec>> {
    let mut children = Vec::new();
    let mut errors = None;

    for field in fields.named {
        match parse_child(field) {
            Ok(Some(child)) => children.push(child),
            Ok(None) => {}
            Err(error) => combine_error(&mut errors, error),
        }
    }

    if let Err(error) = ensure_unique_view_method_names(&children) {
        combine_error(&mut errors, error);
    }
    if let Err(error) = ensure_unique_child_paths(&children) {
        combine_error(&mut errors, error);
    }

    match errors {
        Some(error) => Err(error),
        None => Ok(children),
    }
}

fn ensure_unique_view_method_names(children: &[ChildSpec]) -> Result<()> {
    let mut view_names = HashSet::new();
    let mut errors = None;

    for child in children {
        let view_name = child.view_method_name();
        if !view_names.insert(view_name.clone()) {
            combine_error(
                &mut errors,
                Error::new(
                    child.field_ident.span(),
                    format!("duplicate generated helper name `{view_name}`"),
                ),
            );
        }
    }

    match errors {
        Some(error) => Err(error),
        None => Ok(()),
    }
}

fn ensure_unique_child_paths(children: &[ChildSpec]) -> Result<()> {
    let mut seen_paths = HashMap::new();
    let mut errors = None;

    for child in children {
        let path_value = child.path.value();
        if let Some(first_field) = seen_paths.insert(path_value.clone(), child.field_ident.clone())
        {
            combine_error(
                &mut errors,
                Error::new(
                    child.path.span(),
                    format!(
                        "duplicate child path `{path_value}` for fields `{first_field}` and `{}`",
                        child.field_ident
                    ),
                ),
            );
        }
    }

    match errors {
        Some(error) => Err(error),
        None => Ok(()),
    }
}

/// # Errors
///
/// Returns an error if the field is unnamed or if `#[child(...)]` attributes are missing
/// required parameters or duplicated.
fn parse_child(field: Field) -> Result<Option<ChildSpec>> {
    let field_span = field.span();
    let Some(field_ident) = field.ident else {
        return Err(Error::new(
            field_span,
            "`Composite` supports only named child fields",
        ));
    };

    let mut saw_child = false;
    let mut saw_path = false;
    let mut path = None;
    let mut lift = None;
    let mut extract = None;
    let mut errors = None;

    for attr in &field.attrs {
        if !attr.path().is_ident("child") {
            continue;
        }

        saw_child = true;
        let metas = attr.parse_args_with(Punctuated::<Meta, Token![,]>::parse_terminated)?;
        for meta in metas {
            parse_child_meta(
                meta,
                &mut saw_path,
                &mut path,
                &mut lift,
                &mut extract,
                &mut errors,
            );
        }
    }

    if !saw_child {
        return Ok(None);
    }

    if !saw_path {
        combine_error(&mut errors, Error::new(field_span, "missing `path`"));
    }
    if lift.is_none() {
        combine_error(&mut errors, Error::new(field_span, "missing `lift`"));
    }
    if extract.is_none() {
        combine_error(&mut errors, Error::new(field_span, "missing `extract`"));
    }

    match (path, lift, extract, errors) {
        (_, _, _, Some(error)) => Err(error),
        (Some(path), Some(lift), Some(extract), None) => Ok(Some(ChildSpec {
            field_ident,
            field_ty: field.ty,
            path,
            lift,
            extract,
        })),
        _ => unreachable!("missing child fields must produce an aggregated error"),
    }
}

fn parse_composite_meta(meta: Meta, message_ty: &mut Option<Type>, errors: &mut Option<Error>) {
    match meta {
        Meta::NameValue(name_value) if name_value.path.is_ident("message") => {
            if message_ty.is_some() {
                combine_error(
                    errors,
                    Error::new(name_value.path.span(), "duplicate `message` attribute"),
                );
                return;
            }

            match syn::parse2(name_value.value.into_token_stream()) {
                Ok(parsed) => *message_ty = Some(parsed),
                Err(error) => combine_error(errors, error),
            }
        }
        Meta::NameValue(name_value) => combine_error(
            errors,
            Error::new(name_value.path.span(), "unsupported `composite` attribute"),
        ),
        Meta::Path(path) => combine_error(
            errors,
            Error::new(path.span(), "unsupported `composite` attribute"),
        ),
        Meta::List(meta_list) => combine_error(
            errors,
            Error::new(meta_list.path.span(), "unsupported `composite` attribute"),
        ),
    }
}

fn parse_child_meta(
    meta: Meta,
    saw_path: &mut bool,
    path: &mut Option<LitStr>,
    lift: &mut Option<Expr>,
    extract: &mut Option<Expr>,
    errors: &mut Option<Error>,
) {
    match meta {
        Meta::NameValue(name_value) if name_value.path.is_ident("path") => {
            *saw_path = true;
            set_child_path(path, name_value.path.span(), name_value.value, errors);
        }
        Meta::NameValue(name_value) if name_value.path.is_ident("lift") => {
            set_child_expr(
                lift,
                name_value.path.span(),
                "lift",
                name_value.value,
                errors,
            );
        }
        Meta::NameValue(name_value) if name_value.path.is_ident("extract") => {
            set_child_expr(
                extract,
                name_value.path.span(),
                "extract",
                name_value.value,
                errors,
            );
        }
        Meta::NameValue(name_value) => combine_error(
            errors,
            Error::new(name_value.path.span(), "unsupported `child` attribute"),
        ),
        Meta::Path(path) => combine_error(
            errors,
            Error::new(path.span(), "unsupported `child` attribute"),
        ),
        Meta::List(meta_list) => combine_error(
            errors,
            Error::new(meta_list.path.span(), "unsupported `child` attribute"),
        ),
    }
}

fn set_child_path(slot: &mut Option<LitStr>, span: Span, value: Expr, errors: &mut Option<Error>) {
    if slot.is_some() {
        combine_error(errors, Error::new(span, "duplicate `path` attribute"));
        return;
    }

    match value {
        Expr::Lit(expr_lit) => match expr_lit.lit {
            Lit::Str(path) => *slot = Some(path),
            _ => combine_error(
                errors,
                Error::new(expr_lit.span(), "`path` must be a string literal"),
            ),
        },
        other => combine_error(
            errors,
            Error::new(other.span(), "`path` must be a string literal"),
        ),
    }
}

fn set_child_expr(
    slot: &mut Option<Expr>,
    span: Span,
    name: &str,
    value: Expr,
    errors: &mut Option<Error>,
) {
    if slot.is_some() {
        combine_error(
            errors,
            Error::new(span, format!("duplicate `{name}` attribute")),
        );
        return;
    }

    *slot = Some(value);
}

fn combine_error(errors: &mut Option<Error>, error: Error) {
    if let Some(existing) = errors {
        existing.combine(error);
    } else {
        *errors = Some(error);
    }
}

#[cfg(test)]
mod tests {
    use super::{ensure_unique_view_method_names, parse_composite};
    use crate::spec::ChildSpec;
    use proc_macro2::Span;
    use quote::ToTokens;
    use syn::{DeriveInput, Error, Generics, Ident, Type, parse_quote};

    fn parse_error(input: DeriveInput) -> Error {
        match parse_composite(input) {
            Ok(_) => panic!("expected parse failure"),
            Err(error) => error,
        }
    }

    fn compile_error_string(error: &Error) -> String {
        error.to_compile_error().to_string()
    }

    fn child(field_ident: Ident) -> ChildSpec {
        ChildSpec {
            field_ident,
            field_ty: parse_quote!(ChildModel),
            path: parse_quote!("child"),
            lift: parse_quote!(ParentMsg::Child),
            extract: parse_quote!(ParentMsg::into_child),
        }
    }

    #[test]
    fn parses_valid_composite_spec() {
        let input: DeriveInput = parse_quote! {
            #[derive(Composite)]
            #[composite(message = ParentMsg)]
            struct Parent<T> {
                #[child(path = "left", lift = ParentMsg::Left, extract = ParentMsg::into_left)]
                left: LeftChild<T>,
                ignored: usize,
            }
        };

        let spec = parse_composite(input).expect("expected valid composite");
        let expected_message: Type = parse_quote!(ParentMsg);
        let expected_generics: Generics = parse_quote!(<T>);
        assert_eq!(spec.struct_ident, Ident::new("Parent", Span::call_site()));
        assert_eq!(spec.children.len(), 1);
        assert_eq!(
            spec.message_ty.to_token_stream().to_string(),
            expected_message.to_token_stream().to_string()
        );
        assert_eq!(
            spec.generics.to_token_stream().to_string(),
            expected_generics.to_token_stream().to_string()
        );
        assert_eq!(
            spec.children[0].field_ident,
            Ident::new("left", Span::call_site())
        );
    }

    #[test]
    fn reports_missing_message_on_derive_attribute() {
        let input: DeriveInput = parse_quote! {
            #[derive(Composite)]
            struct Parent {
                #[child(path = "child", lift = ParentMsg::Child, extract = ParentMsg::into_child)]
                child: ChildModel,
            }
        };

        assert_eq!(parse_error(input).to_string(), "missing `message`");
    }

    #[test]
    fn aggregates_duplicate_and_unsupported_child_attributes() {
        let input: DeriveInput = parse_quote! {
            #[derive(Composite)]
            #[composite(message = ParentMsg)]
            struct Parent {
                #[child(path = "left", path = "right", extra = true, lift = ParentMsg::Child, extract = ParentMsg::into_child)]
                child: ChildModel,
            }
        };

        let error = parse_error(input);
        let errors = compile_error_string(&error);
        assert!(errors.contains("duplicate `path` attribute"));
        assert!(errors.contains("unsupported `child` attribute"));
    }

    #[test]
    fn aggregates_duplicate_view_helper_names() {
        let children = [
            child(Ident::new("child", Span::call_site())),
            child(Ident::new_raw("child", Span::call_site())),
        ];

        let error = ensure_unique_view_method_names(&children).expect_err("expected collision");
        assert_eq!(
            error.to_string(),
            "duplicate generated helper name `child_view`"
        );
    }

    #[test]
    fn rejects_duplicate_sibling_child_paths() {
        let input: DeriveInput = parse_quote! {
            #[derive(Composite)]
            #[composite(message = ParentMsg)]
            struct Parent {
                #[child(path = "shared", lift = ParentMsg::First, extract = ParentMsg::into_first)]
                first: FirstChild,
                #[child(path = "shared", lift = ParentMsg::Second, extract = ParentMsg::into_second)]
                second: SecondChild,
            }
        };

        assert_eq!(
            parse_error(input).to_string(),
            "duplicate child path `shared` for fields `first` and `second`"
        );
    }

    #[test]
    fn rejects_non_literal_child_path() {
        let input: DeriveInput = parse_quote! {
            #[derive(Composite)]
            #[composite(message = ParentMsg)]
            struct Parent {
                #[child(path = make_path(), lift = ParentMsg::Child, extract = ParentMsg::into_child)]
                child: ChildModel,
            }
        };

        assert_eq!(
            parse_error(input).to_string(),
            "`path` must be a string literal"
        );
    }

    #[test]
    fn rejects_unsupported_composite_attributes() {
        let input: DeriveInput = parse_quote! {
            #[derive(Composite)]
            #[composite(message = ParentMsg, mode = "strict")]
            struct Parent {
                #[child(path = "child", lift = ParentMsg::Child, extract = ParentMsg::into_child)]
                child: ChildModel,
            }
        };

        assert_eq!(
            parse_error(input).to_string(),
            "unsupported `composite` attribute"
        );
    }
}
