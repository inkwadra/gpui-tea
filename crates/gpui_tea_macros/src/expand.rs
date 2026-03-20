use crate::spec::{ChildSpec, CompositeSpec};
use quote::quote;
use syn::Type;

pub(crate) fn expand_composite(spec: CompositeSpec) -> proc_macro2::TokenStream {
    let CompositeSpec {
        struct_ident,
        generics,
        message_ty,
        children,
    } = spec;

    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();
    let binding_methods = children
        .iter()
        .map(|child| child_bindings_method(child, &message_ty));
    let init_body = init_body(&children);
    let update_steps = children.iter().map(update_step);
    let subscription_steps = children.iter().map(subscription_step);
    let view_methods = children.iter().map(|child| view_method(child, &message_ty));

    quote! {
        impl #impl_generics #struct_ident #ty_generics #where_clause {
            #(#binding_methods)*

            #[doc(hidden)]
            fn __composite_init(
                &mut self,
                cx: &mut ::gpui::App,
                scope: &::gpui_tea::ModelContext<#message_ty>,
            ) -> ::gpui_tea::Command<#message_ty> {
                #init_body
            }

            #[doc(hidden)]
            fn __composite_update(
                &mut self,
                msg: #message_ty,
                cx: &mut ::gpui::App,
                scope: &::gpui_tea::ModelContext<#message_ty>,
            ) -> ::core::result::Result<::gpui_tea::Command<#message_ty>, #message_ty> {
                let msg = msg;
                #(#update_steps)*
                ::core::result::Result::Err(msg)
            }

            #[doc(hidden)]
            fn __composite_subscriptions(
                &self,
                cx: &mut ::gpui::App,
                scope: &::gpui_tea::ModelContext<#message_ty>,
            ) -> ::gpui_tea::Subscriptions<#message_ty> {
                let mut __subscriptions = ::gpui_tea::Subscriptions::none();
                #(#subscription_steps)*
                __subscriptions
            }

            #(#view_methods)*
        }
    }
}

fn init_body(children: &[ChildSpec]) -> proc_macro2::TokenStream {
    if children.is_empty() {
        return quote!(::gpui_tea::Command::none());
    }

    let child_inits = children.iter().map(init_expr);
    quote!(::gpui_tea::Command::batch([#(#child_inits),*]))
}

fn child_msg_ty(child: &ChildSpec) -> proc_macro2::TokenStream {
    let field_ty = &child.field_ty;
    quote!(<#field_ty as ::gpui_tea::Model>::Msg)
}

fn child_bindings_method(child: &ChildSpec, message_ty: &Type) -> proc_macro2::TokenStream {
    let bindings_method_ident = child.bindings_method_ident();
    let child_msg_ty = child_msg_ty(child);
    let lift = &child.lift;
    let extract = &child.extract;

    quote! {
        #[doc(hidden)]
        fn #bindings_method_ident() -> (
            fn(#child_msg_ty) -> #message_ty,
            fn(#message_ty) -> ::core::result::Result<#child_msg_ty, #message_ty>,
        ) {
            (#lift, #extract)
        }
    }
}

fn init_expr(child: &ChildSpec) -> proc_macro2::TokenStream {
    let bindings_method_ident = child.bindings_method_ident();
    let field_ident = &child.field_ident;
    let path = &child.path;

    quote! {{
        let (__lift, _) = Self::#bindings_method_ident();
        let __child_scope = scope.scope(#path, __lift);
        __child_scope.init(&mut self.#field_ident, cx)
    }}
}

fn update_step(child: &ChildSpec) -> proc_macro2::TokenStream {
    let bindings_method_ident = child.bindings_method_ident();
    let field_ident = &child.field_ident;
    let path = &child.path;

    quote! {
        let (__lift, __extract) = Self::#bindings_method_ident();
        let __child_scope = scope.scope(#path, __lift);
        let msg = match __extract(msg) {
            ::core::result::Result::Ok(child_msg) => {
                return ::core::result::Result::Ok(
                    __child_scope.update(&mut self.#field_ident, child_msg, cx),
                );
            }
            ::core::result::Result::Err(msg) => msg,
        };
    }
}

fn subscription_step(child: &ChildSpec) -> proc_macro2::TokenStream {
    let bindings_method_ident = child.bindings_method_ident();
    let field_ident = &child.field_ident;
    let path = &child.path;

    quote! {
        let (__lift, _) = Self::#bindings_method_ident();
        let __child_scope = scope.scope(#path, __lift);
        for __subscription in __child_scope.subscriptions(&self.#field_ident, cx) {
            __subscriptions.push(__subscription).expect(
                "composite child subscriptions must preserve unique scoped keys",
            );
        }
    }
}

fn view_method(child: &ChildSpec, message_ty: &Type) -> proc_macro2::TokenStream {
    let view_ident = child.view_method_ident();
    let bindings_method_ident = child.bindings_method_ident();
    let field_ident = &child.field_ident;
    let path = &child.path;

    quote! {
        #[doc(hidden)]
        fn #view_ident(
            &self,
            window: &mut ::gpui::Window,
            cx: &mut ::gpui::App,
            scope: &::gpui_tea::ModelContext<#message_ty>,
            dispatcher: &::gpui_tea::Dispatcher<#message_ty>,
        ) -> ::gpui_tea::View {
            let (__lift, _) = Self::#bindings_method_ident();
            let __child_scope = scope.scope(#path, __lift);
            __child_scope.view(&self.#field_ident, window, cx, dispatcher)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::expand_composite;
    use crate::spec::{ChildSpec, CompositeSpec};
    use quote::ToTokens;
    use syn::parse_quote;

    #[test]
    fn expansion_emits_shared_bindings_helper_per_child() {
        let spec = CompositeSpec {
            struct_ident: parse_quote!(Parent),
            generics: parse_quote!(<T>),
            message_ty: parse_quote!(ParentMsg),
            children: vec![ChildSpec {
                field_ident: parse_quote!(left),
                field_ty: parse_quote!(LeftChild<T>),
                path: parse_quote!("left"),
                lift: parse_quote!(ParentMsg::Left),
                extract: parse_quote!(ParentMsg::into_left),
            }],
        };

        let tokens = expand_composite(spec).to_token_stream().to_string();

        assert!(tokens.contains("__left_composite_bindings"));
        assert!(tokens.contains("let (__lift , _) = Self :: __left_composite_bindings () ;"));
        assert!(
            tokens.contains("let (__lift , __extract) = Self :: __left_composite_bindings () ;")
        );
    }
}
