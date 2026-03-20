//! Test or Example
#[cfg(feature = "tracing")]
#[path = "utils/ui.rs"]
mod ui;

#[cfg(not(feature = "tracing"))]
fn main() {
    eprintln!(
        "Enable the `tracing` feature to run `RUST_LOG=debug cargo run -p gpui_tea --example telemetry --features tracing`."
    );
}

#[cfg(feature = "tracing")]
mod enabled {
    use super::ui;
    use gpui::{
        App, Application, Bounds, Window, WindowBounds, WindowOptions, div, prelude::*, px, rgba,
        size,
    };
    use gpui_tea::{
        Command, Dispatcher, IntoView, Model, ModelContext, Program, ProgramConfig, View,
        observe_tracing_telemetry,
    };
    use std::time::Duration;

    #[derive(Clone, Copy)]
    pub(super) enum Msg {
        RunSequence,
        RunCancelDemo,
        Loaded(u32),
        CancelTelemetryLoad,
        Cancel,
        Reset,
    }

    pub(super) struct TelemetryModel {
        last_loaded: Option<u32>,
        runs: u32,
    }

    impl TelemetryModel {
        fn last_loaded_label(&self) -> String {
            self.last_loaded
                .map_or_else(|| String::from("none yet"), |value| value.to_string())
        }

        fn status_label(&self) -> &'static str {
            self.last_loaded
                .map_or("Waiting for load", |_| "Value applied")
        }

        fn status_tone(&self) -> ui::BannerTone {
            self.last_loaded
                .map_or(ui::BannerTone::Warning, |_| ui::BannerTone::Success)
        }

        fn cancel_hint(&self) -> &'static str {
            if self.runs == 0 {
                "Start any sequence to generate tracing events."
            } else {
                "Use cancel actions to inspect keyed cancellation and stale completion handling."
            }
        }
    }

    impl Model for TelemetryModel {
        type Msg = Msg;

        fn update(
            &mut self,
            msg: Self::Msg,
            _cx: &mut App,
            _scope: &ModelContext<Self::Msg>,
        ) -> Command<Self::Msg> {
            match msg {
                Msg::RunSequence => {
                    self.runs += 1;
                    let run = self.runs;

                    Command::batch([
                        Command::background_keyed("telemetry-load", move |_| async move {
                            std::thread::sleep(Duration::from_millis(600));
                            Some(Msg::Loaded(run * 10))
                        })
                        .label("slow-load"),
                        Command::background_keyed("telemetry-load", move |_| async move {
                            std::thread::sleep(Duration::from_millis(150));
                            Some(Msg::Loaded(run * 10 + 1))
                        })
                        .label("fast-load"),
                    ])
                }
                Msg::RunCancelDemo => {
                    self.runs += 1;
                    let run = self.runs;

                    Command::batch([
                        Command::background_keyed("telemetry-cancel-demo", move |_| async move {
                            std::thread::sleep(Duration::from_millis(700));
                            Some(Msg::Loaded(run * 100))
                        })
                        .label("cancel-demo-load"),
                        Command::background(|_| async move {
                            std::thread::sleep(Duration::from_millis(75));
                            Some(Msg::CancelTelemetryLoad)
                        })
                        .label("cancel-demo-trigger"),
                    ])
                }
                Msg::Loaded(value) => {
                    self.last_loaded = Some(value);
                    Command::none()
                }
                Msg::CancelTelemetryLoad => Command::cancel_key("telemetry-cancel-demo"),
                Msg::Cancel => Command::cancel_key("telemetry-load"),
                Msg::Reset => {
                    self.last_loaded = None;
                    self.runs = 0;
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
            ui::AppFrame::new().child(
                ui::Card::new()
                    .child(header_section())
                    .child(ui::Text::new(
                        ui::TextRole::Caption,
                        "The runtime flow stays GPUI-safe: mount, init, command execution, queue draining, subscription reconciliation, then notify.",
                    ))
                    .child(
                        ui::Banner::new(
                            "Run this example from a terminal with RUST_LOG=debug to inspect the structured tracing stream.",
                        )
                        .tone(ui::BannerTone::Info),
                    )
                    .child(status_section(self))
                    .child(action_section(dispatcher))
                    .child(notes_section()),
            )
            .into_view()
        }
    }

    fn header_section() -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .gap(px(10.0))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(8.0))
                    .child(ui::Text::new(ui::TextRole::Eyebrow, "gpui_tea example"))
                    .child(ui::Text::new(ui::TextRole::Title, "Telemetry + tracing"))
                    .child(ui::Text::new(
                        ui::TextRole::Body,
                        "A compact GPUI demo for queue activity, keyed replacement, cancellation, and structured tracing output.",
                    )),
            )
            .child(ui::Badge::new("Tracing enabled").tone(ui::BannerTone::Info))
    }

    fn status_section(model: &TelemetryModel) -> impl IntoElement {
        section("Current state")
            .child(
                div()
                    .flex()
                    .justify_between()
                    .items_center()
                    .gap(px(12.0))
                    .child(ui::Badge::new(model.status_label()).tone(model.status_tone())),
            )
            .child(ui::DetailRow::new(
                "Last applied load",
                model.last_loaded_label(),
            ))
            .child(ui::DetailRow::new(
                "Sequences started",
                model.runs.to_string(),
            ))
            .child(ui::Text::new(ui::TextRole::Caption, model.cancel_hint()))
    }

    fn action_section(dispatcher: &Dispatcher<Msg>) -> impl IntoElement {
        section("Interactive scenarios")
            .child(ui::Text::new(
                ui::TextRole::Caption,
                "Use the first row to generate replacement and stale-completion events. Use the second row to explicitly cancel keyed work or reset the demo state.",
            ))
            .child(
                ui::action_row()
                    .child(action_button(
                        dispatcher,
                        "run",
                        "Run telemetry sequence",
                        Msg::RunSequence,
                        ui::ButtonVariant::Primary,
                        "the mounted program should accept run requests",
                    ))
                    .child(action_button(
                        dispatcher,
                        "cancel-demo",
                        "Run cancel demo",
                        Msg::RunCancelDemo,
                        ui::ButtonVariant::Secondary,
                        "the mounted program should accept cancel-demo requests",
                    )),
            )
            .child(
                ui::action_row()
                    .child(action_button(
                        dispatcher,
                        "cancel",
                        "Cancel keyed work",
                        Msg::Cancel,
                        ui::ButtonVariant::Outline,
                        "the mounted program should accept cancel requests",
                    ))
                    .child(action_button(
                        dispatcher,
                        "reset",
                        "Reset",
                        Msg::Reset,
                        ui::ButtonVariant::Ghost,
                        "the mounted program should accept reset requests",
                    )),
            )
    }

    fn section(title: &'static str) -> gpui::Div {
        div()
            .flex()
            .flex_col()
            .gap(px(14.0))
            .p(px(16.0))
            .rounded(px(8.0))
            .bg(rgba(0x282c_33ff))
            .border_1()
            .border_color(rgba(0x363c_46ff))
            .child(ui::Text::new(ui::TextRole::Eyebrow, title))
    }

    /// # Panics
    ///
    /// Panics if the mounted program is unavailable and the dispatcher rejects
    /// the demo message, which indicates the example is no longer mounted.
    fn action_button(
        dispatcher: &Dispatcher<Msg>,
        id: &'static str,
        label: &'static str,
        message: Msg,
        variant: ui::ButtonVariant,
        error_context: &'static str,
    ) -> gpui::Stateful<gpui::Div> {
        ui::Button::new(id, label)
            .variant(variant)
            .build()
            .on_click({
                let dispatcher = dispatcher.clone();
                move |_event, _window, _cx| {
                    dispatcher.dispatch(message).expect(error_context);
                }
            })
    }

    fn notes_section() -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .gap(px(10.0))
            .child(
                ui::Banner::new(
                    "Keyed replacement demo: start a sequence and watch fast work replace slow work.",
                )
                .tone(ui::BannerTone::Neutral),
            )
            .child(
                ui::Banner::new(
                    "Cancel demo: produces keyed.canceled first, then a later stale_ignored completion.",
                )
                .tone(ui::BannerTone::Warning),
            )
    }

    pub(super) fn main_impl() {
        let env_filter = tracing_subscriber::EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("debug"));
        let _ = tracing_subscriber::fmt()
            .with_env_filter(env_filter)
            .with_target(false)
            .with_thread_ids(true)
            .compact()
            .try_init();

        Application::new().run(|cx: &mut App| {
            let bounds = Bounds::centered(None, size(px(960.0), px(760.0)), cx);
            let config = ProgramConfig::new()
                .describe_program(|program_id| format!("program:{program_id}"))
                .describe_message(|msg: &Msg| match msg {
                    Msg::RunSequence => String::from("run-sequence"),
                    Msg::RunCancelDemo => String::from("run-cancel-demo"),
                    Msg::Loaded(value) => format!("loaded:{value}"),
                    Msg::CancelTelemetryLoad => String::from("cancel-telemetry-load"),
                    Msg::Cancel => String::from("cancel"),
                    Msg::Reset => String::from("reset"),
                })
                .describe_key(|key| format!("{key:?}"))
                .telemetry_observer(observe_tracing_telemetry::<Msg>);

            cx.open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(bounds)),
                    ..Default::default()
                },
                move |_, cx| {
                    Program::mount_with(
                        TelemetryModel {
                            last_loaded: None,
                            runs: 0,
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
}

#[cfg(feature = "tracing")]
fn main() {
    enabled::main_impl();
}
