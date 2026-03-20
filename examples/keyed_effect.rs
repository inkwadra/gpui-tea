#![allow(missing_docs)]

#[path = "utils/ui.rs"]
mod ui;

use gpui::{App, Application, Bounds, Window, WindowBounds, WindowOptions, prelude::*, px, size};
use gpui_tea::{Command, Dispatcher, IntoView, Model, Program, View};
use std::time::Duration;

#[derive(Clone, Copy)]
enum Msg {
    RunDemo,
    Loaded(&'static str),
    Clear,
}

struct KeyedEffectDemo {
    status: &'static str,
    applied: Vec<&'static str>,
}

impl Model for KeyedEffectDemo {
    type Msg = Msg;

    fn update(&mut self, msg: Self::Msg, _cx: &mut App) -> Command<Self::Msg> {
        match msg {
            Msg::RunDemo => {
                self.status = "scheduled slow and fast work on the same key";

                Command::batch([
                    Command::background_keyed("load-profile", |_| async move {
                        std::thread::sleep(Duration::from_millis(900));
                        Some(Msg::Loaded("slow result"))
                    })
                    .label("slow-load"),
                    Command::background_keyed("load-profile", |_| async move {
                        std::thread::sleep(Duration::from_millis(150));
                        Some(Msg::Loaded("fast result"))
                    })
                    .label("fast-load"),
                ])
            }
            Msg::Loaded(result) => {
                self.status = result;
                self.applied.push(result);
                Command::none()
            }
            Msg::Clear => {
                self.status = "idle";
                self.applied.clear();
                Command::none()
            }
        }
    }

    fn view(
        &self,
        _window: &mut Window,
        _cx: &mut App,
        dispatcher: &Dispatcher<Self::Msg>,
    ) -> View {
        let applied = if self.applied.is_empty() {
            String::from("none yet")
        } else {
            self.applied.join(", ")
        };

        ui::AppFrame::new().child(
            ui::Card::new()
                .child(ui::Text::new(
                    ui::TextRole::Eyebrow,
                    "gpui_tea example",
                ))
                .child(ui::Text::new(
                    ui::TextRole::Title,
                    "Keyed async work",
                ))
                .child(ui::Text::new(
                    ui::TextRole::Body,
                    "Both commands share one key. The latest completion remains authoritative and stale work is ignored.",
                ))
                .child(ui::DetailRow::new("Current status", self.status))
                .child(ui::DetailRow::new("Applied messages", applied))
                .child(
                    ui::action_row()
                        .child(
                            ui::Button::new("run-demo", "Run demo")
                                .variant(ui::ButtonVariant::Primary)
                                .build()
                                .on_click({
                                    let dispatcher = dispatcher.clone();
                                    move |_event, _window, _cx| {
                                        dispatcher
                                            .dispatch(Msg::RunDemo)
                                            .expect("the mounted demo program should accept run requests");
                                    }
                                }),
                        )
                        .child(
                            ui::Button::new("clear", "Clear")
                                .variant(ui::ButtonVariant::Outline)
                                .build()
                                .on_click({
                                    let dispatcher = dispatcher.clone();
                                    move |_event, _window, _cx| {
                                        dispatcher
                                            .dispatch(Msg::Clear)
                                            .expect("the mounted demo program should accept clear requests");
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
                    KeyedEffectDemo {
                        status: "idle",
                        applied: Vec::new(),
                    },
                    cx,
                )
            },
        )
        .unwrap();

        cx.activate(true);
    });
}
