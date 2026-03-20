use gpui::{AnyElement, Empty, IntoElement};
use std::fmt;

/// Render value returned by [`crate::Model::view`].
///
/// `View` is an owned, GPUI-first wrapper around [`gpui::AnyElement`]. It
/// exists to give `gpui_tea` a narrow render boundary without introducing a
/// second rendering model or lifecycle.
#[must_use]
pub struct View(AnyElement);

impl View {
    /// Wrap any GPUI element-like value as a `View`.
    pub fn new(element: impl IntoElement) -> Self {
        Self(element.into_any_element())
    }

    /// Wrap an existing [`gpui::AnyElement`] as a `View`.
    pub fn from_any_element(element: AnyElement) -> Self {
        Self(element)
    }

    /// Create an empty view that renders nothing.
    pub fn empty() -> Self {
        Self(Empty.into_any_element())
    }

    /// Consume the view and return the wrapped [`gpui::AnyElement`].
    #[must_use]
    pub fn into_any_element(self) -> AnyElement {
        self.0
    }
}

impl Default for View {
    fn default() -> Self {
        Self::empty()
    }
}

impl IntoElement for View {
    type Element = AnyElement;

    fn into_element(self) -> Self::Element {
        self.0
    }

    fn into_any_element(self) -> AnyElement {
        self.0
    }
}

impl From<AnyElement> for View {
    fn from(element: AnyElement) -> Self {
        Self::from_any_element(element)
    }
}

impl From<View> for AnyElement {
    fn from(view: View) -> Self {
        view.into_any_element()
    }
}

impl fmt::Debug for View {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.debug_tuple("View").field(&"..").finish()
    }
}

/// Extension trait for converting GPUI elements into [`View`].
pub trait IntoView: IntoElement + Sized {
    /// Convert this GPUI element into a [`View`].
    fn into_view(self) -> View {
        View::new(self)
    }
}

impl<T: IntoElement> IntoView for T {}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::div;

    fn accepts_any_element(_element: AnyElement) {}

    #[test]
    fn new_round_trips_through_into_element() {
        let view = View::new(div());
        accepts_any_element(view.into_element());
    }

    #[test]
    fn empty_converts_to_any_element() {
        let view = View::empty();
        accepts_any_element(view.into_any_element());
    }

    #[test]
    fn any_element_round_trips() {
        let any_element = div().into_any_element();
        let round_tripped = View::from_any_element(any_element).into_any_element();
        accepts_any_element(round_tripped);
    }

    #[test]
    fn into_view_works_for_gpui_elements() {
        let view = div().into_view();
        accepts_any_element(view.into_any_element());
    }
}
