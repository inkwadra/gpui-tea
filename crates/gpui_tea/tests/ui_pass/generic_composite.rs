use gpui::{App, Window};
use gpui_tea::{Composite, Dispatcher, IntoView, Model, ModelContext, View};

#[derive(Clone, Debug)]
enum Msg<M> {
    Child(M),
}

impl<M> Msg<M> {
    fn into_child(self) -> Result<M, Self> {
        match self {
            Self::Child(m) => Ok(m),
        }
    }
}

struct Child<T>(T);

impl<T: 'static> Model for Child<T> {
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

type ParentMsg = Msg<()>;

#[derive(Composite)]
#[composite(message = ParentMsg)]
struct GenericParent<T: 'static> {
    #[child(path = "child", lift = Msg::Child, extract = Msg::into_child)]
    child: Child<T>,
}

fn main() {}
