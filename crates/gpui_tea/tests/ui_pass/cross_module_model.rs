#![deny(warnings)]
mod parent {
    use gpui_tea::{Command, Composite, Dispatcher, IntoView, Model, ModelContext, View};

    #[derive(Clone, Debug)]
    pub(crate) enum ChildMsg {
        Ping,
    }

    pub(crate) struct Child;

    impl Model for Child {
        type Msg = ChildMsg;

        fn update(
            &mut self,
            msg: Self::Msg,
            _cx: &mut gpui::App,
            _scope: &ModelContext<Self::Msg>,
        ) -> Command<Self::Msg> {
            match msg {
                ChildMsg::Ping => Command::none(),
            }
        }

        fn view(
            &self,
            _window: &mut gpui::Window,
            _cx: &mut gpui::App,
            _scope: &ModelContext<Self::Msg>,
            _dispatcher: &Dispatcher<Self::Msg>,
        ) -> View {
            gpui::div().into_view()
        }
    }

    #[derive(Clone, Debug)]
    pub(crate) enum ParentMsg {
        Child(ChildMsg),
    }

    impl ParentMsg {
        fn into_child(self) -> Result<ChildMsg, Self> {
            match self {
                Self::Child(msg) => Ok(msg),
            }
        }
    }

    #[derive(Composite)]
    #[composite(message = ParentMsg)]
    pub(crate) struct Parent {
        #[child(path = "child", lift = ParentMsg::Child, extract = ParentMsg::into_child)]
        pub(crate) child: Child,
    }
}

mod model {
    use crate::parent::{Parent, ParentMsg};
    use gpui::{App, Window};
    use gpui_tea::{Command, Dispatcher, Model, ModelContext, Subscriptions, View};

    impl Model for Parent {
        type Msg = ParentMsg;

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
            window: &mut Window,
            cx: &mut App,
            scope: &ModelContext<Self::Msg>,
            dispatcher: &Dispatcher<Self::Msg>,
        ) -> View {
            self.child_view(window, cx, scope, dispatcher)
        }
    }
}

fn main() {}
