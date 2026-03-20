use gpui::{AnyElement, Empty, IntoElement};
use std::fmt;

#[must_use]
/// Wrap a GPUI element returned from [`crate::Model::view()`].
///
/// `View` gives the TEA runtime a concrete rendering type while still accepting any GPUI element
/// that implements [`IntoElement`].
pub struct View(AnyElement);

impl View {
    /// Wrap `element` as a [`View`].
    ///
    /// # Arguments
    ///
    /// * `element` - Any value that can be converted into a GPUI element.
    ///
    /// # Returns
    ///
    /// A new [`View`] containing the element.
    pub fn new(element: impl IntoElement) -> Self {
        Self(element.into_any_element())
    }

    /// Wrap an existing [`AnyElement`] as a [`View`].
    ///
    /// # Arguments
    ///
    /// * `element` - The [`AnyElement`] to wrap.
    ///
    /// # Returns
    ///
    /// A new [`View`].
    pub fn from_any_element(element: AnyElement) -> Self {
        Self(element)
    }

    /// Return an empty view.
    ///
    /// # Returns
    ///
    /// A [`View`] containing an [`Empty`] element.
    pub fn empty() -> Self {
        Self(Empty.into_any_element())
    }

    /// Convert this view back into the underlying [`AnyElement`].
    ///
    /// # Returns
    ///
    /// The wrapped [`AnyElement`].
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

/// Convert a GPUI element into a [`View`].
pub trait IntoView: IntoElement + Sized {
    /// Wrap `self` as a [`View`].
    ///
    /// # Returns
    ///
    /// The resulting [`View`].
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
