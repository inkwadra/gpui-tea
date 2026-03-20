//! Test or Example
#[path = "utils/ui.rs"]
mod ui;

use gpui::{App, Application, Bounds, Window, WindowBounds, WindowOptions, prelude::*, px, size};
use gpui_tea::{Command, Composite, Dispatcher, IntoView, Model, ModelContext, Program, View};

#[derive(Clone, Copy)]
enum CounterMsg {
    Increment,
}

#[derive(Clone, Copy)]
enum Msg {
    Sidebar(CounterMsg),
    Content(CounterMsg),
}

impl Msg {
    fn into_sidebar(self) -> Result<CounterMsg, Self> {
        match self {
            Self::Sidebar(msg) => Ok(msg),
            other => Err(other),
        }
    }

    fn into_content(self) -> Result<CounterMsg, Self> {
        match self {
            Self::Content(msg) => Ok(msg),
            other => Err(other),
        }
    }
}

struct CounterChild {
    title: &'static str,
    value: i32,
}

impl Model for CounterChild {
    type Msg = CounterMsg;

    fn update(
        &mut self,
        msg: Self::Msg,
        _cx: &mut App,
        _scope: &ModelContext<Self::Msg>,
    ) -> Command<Self::Msg> {
        match msg {
            CounterMsg::Increment => self.value += 1,
        }

        Command::none()
    }

    fn view(
        &self,
        _window: &mut Window,
        _cx: &mut App,
        scope: &ModelContext<Self::Msg>,
        dispatcher: &Dispatcher<Self::Msg>,
    ) -> View {
        let button_id = format!("{}-increment", scope.path());

        ui::Card::new()
            .child(ui::Text::new(ui::TextRole::Title, self.title))
            .child(ui::Metric::new(self.value.to_string()))
            .child(
                ui::Button::new(button_id, "Increment")
                    .variant(ui::ButtonVariant::Primary)
                    .build()
                    .on_click({
                        let dispatcher = dispatcher.clone();
                        move |_event, _window, _cx| {
                            dispatcher
                                .dispatch(CounterMsg::Increment)
                                .expect("the mounted child program should accept increment clicks");
                        }
                    }),
            )
            .into_view()
    }
}

#[derive(Composite)]
#[composite(message = Msg)]
struct NestedDemo {
    #[child(
        path = "sidebar",
        lift = Msg::Sidebar,
        extract = Msg::into_sidebar
    )]
    sidebar: CounterChild,
    #[child(
        path = "content",
        lift = Msg::Content,
        extract = Msg::into_content
    )]
    content: CounterChild,
}

impl Model for NestedDemo {
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
        match self.__composite_update(msg, cx, scope) {
            Ok(command) => command,
            Err(_msg) => Command::none(),
        }
    }

    fn view(
        &self,
        window: &mut Window,
        cx: &mut App,
        scope: &ModelContext<Self::Msg>,
        dispatcher: &Dispatcher<Self::Msg>,
    ) -> View {
        ui::AppFrame::new().child(
            ui::Card::new()
                .child(ui::Text::new(ui::TextRole::Eyebrow, "gpui_tea example"))
                .child(ui::Text::new(ui::TextRole::Title, "Composite models"))
                .child(ui::Text::new(
                    ui::TextRole::Body,
                    "Both child counters reuse the same local messages while keyed work and subscriptions stay isolated by explicit paths.",
                ))
                .child(
                    ui::action_row()
                        .child(self.sidebar_view(window, cx, scope, dispatcher))
                        .child(self.content_view(window, cx, scope, dispatcher)),
                ),
        )
        .into_view()
    }

    fn subscriptions(
        &self,
        cx: &mut App,
        scope: &ModelContext<Self::Msg>,
    ) -> gpui_tea::Subscriptions<Self::Msg> {
        self.__composite_subscriptions(cx, scope)
    }
}

fn main() {
    Application::new().run(|cx: &mut App| {
        let bounds = Bounds::centered(None, size(px(960.0), px(640.0)), cx);

        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                ..Default::default()
            },
            |_, cx| {
                Program::mount(
                    NestedDemo {
                        sidebar: CounterChild {
                            title: "Sidebar counter",
                            value: 0,
                        },
                        content: CounterChild {
                            title: "Content counter",
                            value: 0,
                        },
                    },
                    cx,
                )
            },
        )
        .unwrap();

        cx.activate(true);
    });
}
