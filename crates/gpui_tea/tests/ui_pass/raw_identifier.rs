use gpui::{App, Window};
use gpui_tea::{Composite, Dispatcher, IntoView, Model, ModelContext, View};

#[derive(Clone, Debug)]
enum Msg {
    Child(()),
}

impl Msg {
    fn into_child(self) -> Result<(), Self> {
        match self {
            Self::Child(m) => Ok(m),
        }
    }
}

struct Child;

impl Model for Child {
    type Msg = ();

    fn update(
        &mut self,
        _msg: Self::Msg,
        _cx: &mut App,
        _scope: &ModelContext<Self::Msg>,
    ) -> gpui_tea::Command<Self::Msg> {
        gpui_tea::Command::none()
    }

    fn view(
        &self,
        _window: &mut Window,
        _cx: &mut App,
        _scope: &ModelContext<Self::Msg>,
        _dispatcher: &Dispatcher<Self::Msg>,
    ) -> View {
        gpui::div().into_view()
    }
}

#[derive(Composite)]
#[composite(message = Msg)]
struct Parent {
    #[child(path = "type", lift = Msg::Child, extract = Msg::into_child)]
    r#type: Child,
}

fn main() {}
