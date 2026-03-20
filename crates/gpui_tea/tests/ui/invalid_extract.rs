use gpui::{App, Window, div};
use gpui_tea::{Command, Composite, Dispatcher, IntoView, Model, ModelContext, View};

enum ChildMsg {
    Increment,
}

enum Msg {
    Child(ChildMsg),
}

impl Msg {
    fn into_child(self) -> Option<ChildMsg> {
        match self {
            Self::Child(msg) => Some(msg),
        }
    }
}

struct Child;

impl Model for Child {
    type Msg = ChildMsg;

    fn update(
        &mut self,
        _msg: Self::Msg,
        _cx: &mut App,
        _scope: &ModelContext<Self::Msg>,
    ) -> Command<Self::Msg> {
        Command::none()
    }

    fn view(
        &self,
        _window: &mut Window,
        _cx: &mut App,
        _scope: &ModelContext<Self::Msg>,
        _dispatcher: &Dispatcher<Self::Msg>,
    ) -> View {
        div().into_view()
    }
}

#[derive(Composite)]
#[composite(message = Msg)]
struct Parent {
    #[child(path = "child", lift = Msg::Child, extract = Msg::into_child)]
    child: Child,
}

fn main() {}
