#![allow(missing_docs)]

#[path = "utils/ui.rs"]
mod ui;

use gpui::{App, Application, Bounds, Window, WindowBounds, WindowOptions, prelude::*, px, size};
use gpui_tea::{Command, Dispatcher, IntoView, ModelContext, NestedModel, Program, View};

#[derive(Clone, Copy)]
enum CounterMsg {
    Increment,
}

#[derive(Clone, Copy)]
enum Msg {
    Sidebar(CounterMsg),
    Content(CounterMsg),
}

struct CounterChild {
    title: &'static str,
    value: i32,
}

impl NestedModel for CounterChild {
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
                            let _ = dispatcher.dispatch(CounterMsg::Increment);
                        }
                    }),
            )
            .into_view()
    }
}

struct NestedDemo {
    sidebar: CounterChild,
    content: CounterChild,
}

impl NestedModel for NestedDemo {
    type Msg = Msg;

    fn update(
        &mut self,
        msg: Self::Msg,
        cx: &mut App,
        scope: &ModelContext<Self::Msg>,
    ) -> Command<Self::Msg> {
        match msg {
            Msg::Sidebar(child_msg) => {
                scope
                    .scope("sidebar", Msg::Sidebar)
                    .update(&mut self.sidebar, child_msg, cx)
            }
            Msg::Content(child_msg) => {
                scope
                    .scope("content", Msg::Content)
                    .update(&mut self.content, child_msg, cx)
            }
        }
    }

    fn view(
        &self,
        window: &mut Window,
        cx: &mut App,
        scope: &ModelContext<Self::Msg>,
        dispatcher: &Dispatcher<Self::Msg>,
    ) -> View {
        let sidebar = scope.scope("sidebar", Msg::Sidebar);
        let content = scope.scope("content", Msg::Content);

        ui::AppFrame::new().child(
            ui::Card::new()
                .child(ui::Text::new(ui::TextRole::Eyebrow, "gpui_tea example"))
                .child(ui::Text::new(ui::TextRole::Title, "Nested models"))
                .child(ui::Text::new(
                    ui::TextRole::Body,
                    "Both child counters reuse the same local messages while keyed work and subscriptions stay isolated by explicit paths.",
                ))
                .child(
                    ui::action_row()
                        .child(sidebar.view(&self.sidebar, window, cx, dispatcher))
                        .child(content.view(&self.content, window, cx, dispatcher)),
                ),
        )
        .into_view()
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
