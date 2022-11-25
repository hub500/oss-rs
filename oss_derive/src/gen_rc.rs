use quote::ToTokens;
use syn::{
    parse::{Parse, ParseStream, Result},
    parse_quote,
    visit_mut::{self, VisitMut},
    Ident, ItemImpl,
};

pub struct GenImpl {
    inner: ItemImpl,
}

impl Parse for GenImpl {
    fn parse(input: ParseStream) -> Result<Self> {
        Ok(Self {
            inner: input.parse()?,
        })
    }
}

impl ToTokens for GenImpl {
    fn to_tokens(&self, tokens: &mut proc_macro2::TokenStream) {
        self.inner.to_tokens(tokens);

        self.extend_to_tokens(tokens);
    }
}

impl GenImpl {
    fn extend_to_tokens(&self, tokens: &mut proc_macro2::TokenStream) {
        let mut item = self.inner.clone();
        ReplaceArc.visit_item_impl_mut(&mut item);

        item.to_tokens(tokens);
    }
}

struct ReplaceArc;

impl VisitMut for ReplaceArc {
    fn visit_item_impl_mut(&mut self, i: &mut ItemImpl) {
        i.attrs.push(parse_quote! { #[cfg(feature = "blocking")] });
        visit_mut::visit_item_impl_mut(self, i);
    }

    fn visit_ident_mut(&mut self, i: &mut Ident) {
        if i.to_owned() == "ArcPointer" {
            *i = parse_quote! {RcPointer};
        }
        visit_mut::visit_ident_mut(self, i);
    }
}
