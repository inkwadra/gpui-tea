use quote::format_ident;
use syn::{Expr, Generics, Ident, LitStr, Type};

pub(crate) struct CompositeSpec {
    pub(crate) struct_ident: Ident,
    pub(crate) generics: Generics,
    pub(crate) message_ty: Type,
    pub(crate) children: Vec<ChildSpec>,
}

pub(crate) struct ChildSpec {
    pub(crate) field_ident: Ident,
    pub(crate) field_ty: Type,
    pub(crate) path: LitStr,
    pub(crate) lift: Expr,
    pub(crate) extract: Expr,
}

impl ChildSpec {
    pub(crate) fn bindings_method_ident(&self) -> Ident {
        format_ident!("__{}_composite_bindings", self.sanitized_field_name())
    }

    pub(crate) fn view_method_ident(&self) -> Ident {
        format_ident!("{}", self.view_method_name())
    }

    pub(crate) fn view_method_name(&self) -> String {
        format!("{}_view", self.sanitized_field_name())
    }

    fn sanitized_field_name(&self) -> String {
        let mut base = self.field_ident.to_string();
        if let Some(stripped) = base.strip_prefix("r#") {
            base = stripped.to_owned();
        }
        base
    }
}

#[cfg(test)]
mod tests {
    use super::ChildSpec;
    use proc_macro2::Span;
    use quote::ToTokens;
    use syn::{Expr, Ident, LitStr, Type, parse_quote};

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
    fn view_method_name_strips_raw_identifier_prefix() {
        let child = child(Ident::new_raw("type", Span::call_site()));
        assert_eq!(child.view_method_name(), "type_view");
    }

    #[test]
    fn bindings_method_name_strips_raw_identifier_prefix() {
        let child = child(Ident::new_raw("type", Span::call_site()));
        assert_eq!(
            child.bindings_method_ident().to_string(),
            "__type_composite_bindings"
        );
    }

    #[test]
    fn child_spec_keeps_original_field_components() {
        let child = child(Ident::new("sidebar", Span::call_site()));
        let field_ty: Type = parse_quote!(ChildModel);
        let path = LitStr::new("child", Span::call_site());
        let lift: Expr = parse_quote!(ParentMsg::Child);
        let extract: Expr = parse_quote!(ParentMsg::into_child);

        assert_eq!(child.field_ident, Ident::new("sidebar", Span::call_site()));
        assert_eq!(
            child.field_ty.to_token_stream().to_string(),
            field_ty.to_token_stream().to_string()
        );
        assert_eq!(
            child.path.to_token_stream().to_string(),
            path.to_token_stream().to_string()
        );
        assert_eq!(
            child.lift.to_token_stream().to_string(),
            lift.to_token_stream().to_string()
        );
        assert_eq!(
            child.extract.to_token_stream().to_string(),
            extract.to_token_stream().to_string()
        );
    }
}
