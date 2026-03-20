#![allow(missing_docs)]

#[path = "utils/ui.rs"]
mod ui;

use gpui::{App, Application, Bounds, Window, WindowBounds, WindowOptions, prelude::*, px, size};
use gpui_tea::{
    Command, Dispatcher, IntoView, Model, Program, SubHandle, Subscription, Subscriptions, View,
};

#[derive(Clone, Copy)]
enum Msg {
    UseAlpha,
    UseBeta,
    Clear,
    FromSubscription(&'static str),
}

struct SubscriptionDemo {
    active_key: Option<&'static str>,
    delivered: Vec<&'static str>,
}

impl Model for SubscriptionDemo {
    type Msg = Msg;

    fn update(&mut self, msg: Self::Msg, _cx: &mut App) -> Command<Self::Msg> {
        match msg {
            Msg::UseAlpha => self.active_key = Some("alpha"),
            Msg::UseBeta => self.active_key = Some("beta"),
            Msg::Clear => {
                self.active_key = None;
                self.delivered.clear();
            }
            Msg::FromSubscription(key) => self.delivered.push(key),
        }

        Command::none()
    }

    fn subscriptions(&self, _cx: &mut App) -> Subscriptions<Self::Msg> {
        let Some(key) = self.active_key else {
            return Subscriptions::none();
        };

        Subscriptions::one(
            Subscription::new(key, move |cx| {
                cx.dispatch(Msg::FromSubscription(key)).expect(
                    "the mounted subscription demo program should be alive while building subscriptions",
                );
                SubHandle::None
            })
            .label(match key {
                "alpha" => "alpha-subscription",
                "beta" => "beta-subscription",
                _ => "subscription",
            }),
        )
    }

    fn view(
        &self,
        _window: &mut Window,
        _cx: &mut App,
        dispatcher: &Dispatcher<Self::Msg>,
    ) -> View {
        let delivered = if self.delivered.is_empty() {
            String::from("none")
        } else {
            self.delivered.join(", ")
        };
        let active_key = self.active_key.unwrap_or("none");

        ui::AppFrame::new().child(
            ui::Card::new()
                .child(ui::Text::new(
                    ui::TextRole::Eyebrow,
                    "gpui_tea example",
                ))
                .child(ui::Text::new(
                    ui::TextRole::Title,
                    "Declarative subscriptions",
                ))
                .child(ui::Text::new(
                    ui::TextRole::Body,
                    "Reusing the same key retains the existing handle. Changing the key rebuilds the subscription.",
                ))
                .child(ui::DetailRow::new("Active key", active_key))
                .child(ui::DetailRow::new(
                    "Messages delivered by builders",
                    delivered,
                ))
                .child(
                    ui::action_row()
                        .child(
                            ui::Button::new("use-alpha", "Use alpha")
                                .variant(ui::ButtonVariant::Primary)
                                .build()
                                .on_click({
                                    let dispatcher = dispatcher.clone();
                                    move |_event, _window, _cx| {
                                        dispatcher
                                            .dispatch(Msg::UseAlpha)
                                            .expect("the mounted subscription demo should accept selection changes");
                                    }
                                }),
                        )
                        .child(
                            ui::Button::new("use-beta", "Use beta")
                                .variant(ui::ButtonVariant::Secondary)
                                .build()
                                .on_click({
                                    let dispatcher = dispatcher.clone();
                                    move |_event, _window, _cx| {
                                        dispatcher
                                            .dispatch(Msg::UseBeta)
                                            .expect("the mounted subscription demo should accept selection changes");
                                    }
                                }),
                        )
                        .child(
                            ui::Button::new("clear", "Clear")
                                .variant(ui::ButtonVariant::Ghost)
                                .build()
                                .on_click({
                                    let dispatcher = dispatcher.clone();
                                    move |_event, _window, _cx| {
                                        dispatcher
                                            .dispatch(Msg::Clear)
                                            .expect("the mounted subscription demo should accept clear requests");
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
                    SubscriptionDemo {
                        active_key: None,
                        delivered: Vec::new(),
                    },
                    cx,
                )
            },
        )
        .unwrap();

        cx.activate(true);
    });
}
