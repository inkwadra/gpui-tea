#![deny(warnings)]
use gpui::{App, Window};
use gpui_tea::{Command, Composite, Dispatcher, IntoView, Model, ModelContext, Subscriptions, View};

#[derive(Clone, Debug)]
enum Msg {}

#[derive(Composite)]
#[composite(message = Msg)]
struct EmptyParent {}

impl Model for EmptyParent {
    type Msg = Msg;

    fn init(&mut self, cx: &mut App, scope: &ModelContext<Self::Msg>) -> Command<Self::Msg> {
        self.__composite_init(cx, scope)
    }

    fn update(
        &mut self,
        msg: Self::Msg,
        cx: &mut App,
        scope: &ModelContext<Self::Msg>,
    ) -> Command<Self::Msg> {
        self.__composite_update(msg, cx, scope)
            .unwrap_or_else(|_| Command::none())
    }

    fn subscriptions(
        &self,
        cx: &mut App,
        scope: &ModelContext<Self::Msg>,
    ) -> Subscriptions<Self::Msg> {
        self.__composite_subscriptions(cx, scope)
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

fn main() {}


