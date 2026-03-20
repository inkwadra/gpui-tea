//! Test or Example
#[path = "utils/ui.rs"]
mod ui;

use gpui::{App, Application, Bounds, Window, WindowBounds, WindowOptions, prelude::*, px, size};
use gpui_tea::{
    Command, Dispatcher, IntoView, Model, ModelContext, Program, ProgramConfig, RuntimeEvent, View,
};
use std::time::Duration;

#[derive(Clone, Copy)]
enum Msg {
    RunObservedSequence,
    Loaded(u32),
    Reset,
}

struct ObservedModel {
    last_loaded: Option<u32>,
    run_count: u32,
}

impl Model for ObservedModel {
    type Msg = Msg;

    fn update(
        &mut self,
        msg: Self::Msg,
        _cx: &mut App,
        _scope: &ModelContext<Self::Msg>,
    ) -> Command<Self::Msg> {
        match msg {
            Msg::RunObservedSequence => {
                self.run_count += 1;
                let run = self.run_count;

                Command::batch([
                    Command::background_keyed("observed-load", move |_| async move {
                        std::thread::sleep(Duration::from_millis(700));
                        Some(Msg::Loaded(run * 10))
                    })
                    .label("slow-load"),
                    Command::background_keyed("observed-load", move |_| async move {
                        std::thread::sleep(Duration::from_millis(150));
                        Some(Msg::Loaded(run * 10 + 1))
                    })
                    .label("fast-load"),
                ])
            }
            Msg::Loaded(value) => {
                self.last_loaded = Some(value);
                Command::none()
            }
            Msg::Reset => {
                self.last_loaded = None;
                self.run_count = 0;
                Command::none()
            }
        }
    }

    fn view(
        &self,
        _window: &mut Window,
        _cx: &mut App,
        _scope: &ModelContext<Self::Msg>,
        dispatcher: &Dispatcher<Self::Msg>,
    ) -> View {
        let last_loaded = self
            .last_loaded
            .map_or_else(|| String::from("none yet"), |value| value.to_string());

        ui::AppFrame::new().child(
            ui::Card::new()
                .child(ui::Text::new(
                    ui::TextRole::Eyebrow,
                    "gpui_tea example",
                ))
                .child(ui::Text::new(
                    ui::TextRole::Title,
                    "Runtime observability",
                ))
                .child(ui::Text::new(
                    ui::TextRole::Body,
                    "ProgramConfig attaches runtime hooks for dispatch, keyed work, and effect completion events.",
                ))
                .child(
                    ui::Banner::new(
                        "Run this example from a terminal and watch the RuntimeEvent summaries on stderr.",
                    )
                    .tone(ui::BannerTone::Info),
                )
                .child(ui::DetailRow::new(
                    "Completed loads applied to the model",
                    last_loaded,
                ))
                .child(ui::DetailRow::new(
                    "Sequence runs started",
                    self.run_count.to_string(),
                ))
                .child(
                    ui::action_row()
                        .child(
                            ui::Button::new("run-observed-work", "Run observed work")
                                .variant(ui::ButtonVariant::Primary)
                                .build()
                                .on_click({
                                    let dispatcher = dispatcher.clone();
                                    move |_event, _window, _cx| {
                                        dispatcher
                                            .dispatch(Msg::RunObservedSequence)
                                            .expect("the mounted observed program should accept run requests");
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
                                            .expect("the mounted observed program should accept reset requests");
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
        let config = ProgramConfig::new()
            .describe_message(|msg: &Msg| match msg {
                Msg::RunObservedSequence => String::from("run-observed-sequence"),
                Msg::Loaded(value) => format!("loaded:{value}"),
                Msg::Reset => String::from("reset"),
            })
            .describe_key(|key| format!("{key:?}"))
            .observer(observe_runtime_event);

        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                ..Default::default()
            },
            move |_, cx| {
                Program::mount_with(
                    ObservedModel {
                        last_loaded: None,
                        run_count: 0,
                    },
                    config,
                    cx,
                )
            },
        )
        .unwrap();

        cx.activate(true);
    });
}

fn observe_runtime_event(event: RuntimeEvent<'_, Msg>) {
    match event {
        RuntimeEvent::DispatchAccepted {
            message_description,
        } => {
            eprintln!("dispatch accepted: {}", describe(message_description));
        }
        RuntimeEvent::CommandScheduled {
            label,
            key_description,
            ..
        } => {
            eprintln!(
                "command scheduled: label={}, key={}",
                label.unwrap_or("<none>"),
                describe(key_description)
            );
        }
        RuntimeEvent::KeyedCommandReplaced {
            key_description,
            previous_label,
            next_label,
            ..
        } => {
            eprintln!(
                "keyed command replaced: key={}, previous={}, next={}",
                describe(key_description),
                previous_label.unwrap_or("<none>"),
                next_label.unwrap_or("<none>")
            );
        }
        RuntimeEvent::EffectCompleted {
            label,
            emitted_message,
            message_description,
            ..
        } => {
            eprintln!(
                "effect completed: label={}, emitted_message={}, message={}",
                label.unwrap_or("<none>"),
                emitted_message,
                describe(message_description)
            );
        }
        RuntimeEvent::StaleKeyedCompletionIgnored {
            label,
            key_description,
            message_description,
            ..
        } => {
            eprintln!(
                "stale completion ignored: label={}, key={}, message={}",
                label.unwrap_or("<none>"),
                describe(key_description),
                describe(message_description)
            );
        }
        _ => {}
    }
}

fn describe(value: Option<impl AsRef<str>>) -> String {
    value.map_or_else(|| String::from("<none>"), |value| value.as_ref().to_owned())
}
