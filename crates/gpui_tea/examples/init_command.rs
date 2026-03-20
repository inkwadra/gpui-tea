//! Test or Example
#[path = "utils/ui.rs"]
mod ui;

use gpui::{App, Application, Bounds, Window, WindowBounds, WindowOptions, prelude::*, px, size};
use gpui_tea::{Command, Dispatcher, IntoView, Model, ModelContext, Program, View};

#[derive(Clone, Copy)]
enum Msg {
    BootstrapLoaded,
    Increment,
    Reset,
}

struct BootstrappedCounter {
    status: &'static str,
    value: i32,
}

impl Model for BootstrappedCounter {
    type Msg = Msg;

    fn init(&mut self, _cx: &mut App, _scope: &ModelContext<Self::Msg>) -> Command<Self::Msg> {
        Command::emit(Msg::BootstrapLoaded).label("bootstrap")
    }

    fn update(
        &mut self,
        msg: Self::Msg,
        _cx: &mut App,
        _scope: &ModelContext<Self::Msg>,
    ) -> Command<Self::Msg> {
        match msg {
            Msg::BootstrapLoaded => {
                self.status = "loaded by Model::init";
                self.value = 41;
            }
            Msg::Increment => self.value += 1,
            Msg::Reset => self.value = 41,
        }

        Command::none()
    }

    fn view(
        &self,
        _window: &mut Window,
        _cx: &mut App,
        _scope: &ModelContext<Self::Msg>,
        dispatcher: &Dispatcher<Self::Msg>,
    ) -> View {
        ui::AppFrame::new().child(
            ui::Card::new()
                .child(ui::Text::new(
                    ui::TextRole::Eyebrow,
                    "gpui_tea example",
                ))
                .child(ui::Text::new(
                    ui::TextRole::Title,
                    "Initial command example",
                ))
                .child(ui::Text::new(
                    ui::TextRole::Body,
                    "Model::init schedules a follow-up message before normal user-driven updates begin.",
                ))
                .child(ui::DetailRow::new("Status", self.status))
                .child(ui::Metric::new(self.value.to_string()))
                .child(
                    ui::action_row()
                        .child(
                            ui::Button::new("increment", "Increment")
                                .variant(ui::ButtonVariant::Primary)
                                .build()
                                .on_click({
                                    let dispatcher = dispatcher.clone();
                                    move |_event, _window, _cx| {
                                        dispatcher
                                            .dispatch(Msg::Increment)
                                            .expect("the mounted counter program should accept increment clicks");
                                    }
                                }),
                        )
                        .child(
                            ui::Button::new("reset", "Reset")
                                .variant(ui::ButtonVariant::Outline)
                                .build()
                                .on_click({
                                    let dispatcher = dispatcher.clone();
                                    move |_event, _window, _cx| {
                                        dispatcher
                                            .dispatch(Msg::Reset)
                                            .expect("the mounted counter program should accept reset clicks");
                                    }
                                }),
                        ),
                ),
        )
        .into_view()
    }
}

fn main() {
    Application::new().run(|cx: &mut App| {
        let bounds = Bounds::centered(None, size(px(800.0), px(600.0)), cx);

        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                ..Default::default()
            },
            |_, cx| {
                Program::mount(
                    BootstrappedCounter {
                        status: "waiting for init",
                        value: 0,
                    },
                    cx,
                )
            },
        )
        .unwrap();

        cx.activate(true);
    });
}
