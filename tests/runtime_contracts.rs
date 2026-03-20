//! Verify the published runtime contracts exposed by `gpui_tea`.
#![allow(clippy::missing_panics_doc, clippy::too_many_lines)]

use futures::channel::oneshot;
use gpui::{App, Entity, TestAppContext, Window, div};
use gpui_tea::{
    ChildPath, Command, CommandKind, Dispatcher, IntoView, Model, ModelContext, ModelExt,
    NestedModel, Program, ProgramConfig, RuntimeEvent, SubHandle, Subscription, Subscriptions,
    View,
};
use std::{
    collections::VecDeque,
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
};

#[gpui::test]
fn mount_runs_init_once_executes_initial_command_and_builds_initial_subscription_once(
    cx: &mut TestAppContext,
) {
    #[derive(Clone, Debug, PartialEq, Eq)]
    enum Msg {
        InitLoaded(i32),
        SubscriptionReady(&'static str),
    }

    struct LifecycleModel {
        state: i32,
        init_calls: usize,
        delivered: Vec<&'static str>,
        subscription_builds: Arc<AtomicUsize>,
    }

    impl Model for LifecycleModel {
        type Msg = Msg;

        fn init(&mut self, _cx: &mut App) -> Command<Self::Msg> {
            self.init_calls += 1;
            Command::emit(Msg::InitLoaded(41)).label("init")
        }

        fn update(&mut self, msg: Self::Msg, _cx: &mut App) -> Command<Self::Msg> {
            match msg {
                Msg::InitLoaded(value) => {
                    self.state = value;
                    Command::none()
                }
                Msg::SubscriptionReady(message) => {
                    self.delivered.push(message);
                    Command::none()
                }
            }
        }

        fn subscriptions(&self, _cx: &mut App) -> Subscriptions<Self::Msg> {
            let subscription_builds = self.subscription_builds.clone();

            Subscriptions::one(Subscription::new("lifecycle", move |cx| {
                subscription_builds.fetch_add(1, Ordering::SeqCst);
                cx.dispatch(Msg::SubscriptionReady("ready")).unwrap();
                SubHandle::None
            }))
        }

        fn view(
            &self,
            _window: &mut Window,
            _cx: &mut App,
            _dispatcher: &Dispatcher<Self::Msg>,
        ) -> View {
            div().into_view()
        }
    }

    let subscription_builds = Arc::new(AtomicUsize::new(0));
    let program: Entity<Program<LifecycleModel>> = cx.update(|cx| {
        LifecycleModel {
            state: 0,
            init_calls: 0,
            delivered: Vec::new(),
            subscription_builds: subscription_builds.clone(),
        }
        .into_program(cx)
    });

    cx.run_until_parked();

    program.read_with(cx, |program, _cx| {
        assert_eq!(program.model().init_calls, 1);
        assert_eq!(program.model().state, 41);
        assert_eq!(program.model().delivered, vec!["ready"]);
    });
    assert_eq!(subscription_builds.load(Ordering::SeqCst), 1);
}

#[gpui::test]
fn dispatcher_preserves_fifo_delivery_for_enqueued_messages(cx: &mut TestAppContext) {
    #[derive(Clone, Debug, PartialEq, Eq)]
    enum Msg {
        Push(&'static str),
    }

    struct QueueModel {
        trace: Vec<&'static str>,
    }

    impl Model for QueueModel {
        type Msg = Msg;

        fn update(&mut self, msg: Self::Msg, _cx: &mut App) -> Command<Self::Msg> {
            match msg {
                Msg::Push(entry) => self.trace.push(entry),
            }

            Command::none()
        }

        fn view(
            &self,
            _window: &mut Window,
            _cx: &mut App,
            _dispatcher: &Dispatcher<Self::Msg>,
        ) -> View {
            div().into_view()
        }
    }

    let program: Entity<Program<QueueModel>> =
        cx.update(|cx| QueueModel { trace: Vec::new() }.into_program(cx));
    let dispatcher = program.read_with(cx, |program, _cx| program.dispatcher());

    dispatcher.dispatch(Msg::Push("first")).unwrap();
    dispatcher.dispatch(Msg::Push("second")).unwrap();
    dispatcher.dispatch(Msg::Push("third")).unwrap();
    cx.run_until_parked();

    program.read_with(cx, |program, _cx| {
        assert_eq!(program.model().trace, vec!["first", "second", "third"]);
    });
}

#[gpui::test]
fn program_accepts_models_that_render_through_view(cx: &mut TestAppContext) {
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum Msg {
        Increment,
    }

    struct ViewModel {
        updates: usize,
    }

    impl Model for ViewModel {
        type Msg = Msg;

        fn update(&mut self, msg: Self::Msg, _cx: &mut App) -> Command<Self::Msg> {
            match msg {
                Msg::Increment => {
                    self.updates += 1;
                    Command::none()
                }
            }
        }

        fn view(
            &self,
            _window: &mut Window,
            _cx: &mut App,
            _dispatcher: &Dispatcher<Self::Msg>,
        ) -> View {
            View::new(div())
        }
    }

    let program: Entity<Program<ViewModel>> =
        cx.update(|cx| ViewModel { updates: 0 }.into_program(cx));
    let dispatcher = program.read_with(cx, |program, _cx| program.dispatcher());

    dispatcher.dispatch(Msg::Increment).unwrap();
    cx.run_until_parked();

    program.read_with(cx, |program, _cx| {
        assert_eq!(program.model().updates, 1);
    });
}

#[gpui::test]
fn reentrant_emits_append_to_queue_without_recursive_update_and_warn_on_growth(
    cx: &mut TestAppContext,
) {
    #[derive(Clone, Debug, PartialEq, Eq)]
    enum Msg {
        Start,
        FollowUp(&'static str),
    }

    #[derive(Clone, Debug, PartialEq, Eq)]
    enum QueueEvent {
        Started(usize),
        Warning { queued: usize, threshold: usize },
        Finished { processed: usize, remaining: usize },
    }

    struct ReentrantModel {
        trace: Vec<&'static str>,
        depth: usize,
        max_depth: usize,
    }

    impl Model for ReentrantModel {
        type Msg = Msg;

        fn update(&mut self, msg: Self::Msg, _cx: &mut App) -> Command<Self::Msg> {
            self.depth += 1;
            self.max_depth = self.max_depth.max(self.depth);

            let command = match msg {
                Msg::Start => {
                    self.trace.push("start");
                    Command::batch([
                        Command::emit(Msg::FollowUp("follow-up-1")),
                        Command::emit(Msg::FollowUp("follow-up-2")),
                    ])
                }
                Msg::FollowUp(entry) => {
                    self.trace.push(entry);
                    Command::none()
                }
            };

            self.depth -= 1;
            command
        }

        fn view(
            &self,
            _window: &mut Window,
            _cx: &mut App,
            _dispatcher: &Dispatcher<Self::Msg>,
        ) -> View {
            div().into_view()
        }
    }

    let queue_events = Arc::new(Mutex::new(Vec::new()));
    let config = ProgramConfig::default()
        .queue_warning_threshold(1)
        .observer({
            let queue_events = queue_events.clone();
            move |event: RuntimeEvent<'_, Msg>| {
                let record = match event {
                    RuntimeEvent::QueueDrainStarted { queued } => Some(QueueEvent::Started(queued)),
                    RuntimeEvent::QueueWarning { queued, threshold } => {
                        Some(QueueEvent::Warning { queued, threshold })
                    }
                    RuntimeEvent::QueueDrainFinished {
                        processed,
                        remaining,
                    } => Some(QueueEvent::Finished {
                        processed,
                        remaining,
                    }),
                    _ => None,
                };

                if let Some(record) = record {
                    queue_events.lock().unwrap().push(record);
                }
            }
        });

    let program: Entity<Program<ReentrantModel>> = cx.update(|cx| {
        ReentrantModel {
            trace: Vec::new(),
            depth: 0,
            max_depth: 0,
        }
        .into_program_with(config, cx)
    });
    let dispatcher = program.read_with(cx, |program, _cx| program.dispatcher());

    dispatcher.dispatch(Msg::Start).unwrap();
    cx.run_until_parked();

    program.read_with(cx, |program, _cx| {
        assert_eq!(
            program.model().trace,
            vec!["start", "follow-up-1", "follow-up-2"]
        );
        assert_eq!(program.model().max_depth, 1);
    });
    assert_eq!(
        queue_events.lock().unwrap().as_slice(),
        &[
            QueueEvent::Started(1),
            QueueEvent::Warning {
                queued: 2,
                threshold: 1,
            },
            QueueEvent::Finished {
                processed: 3,
                remaining: 0,
            },
        ]
    );
}

#[gpui::test]
fn batch_executes_as_a_linear_sequence_and_skips_none(cx: &mut TestAppContext) {
    #[derive(Clone, Debug, PartialEq, Eq)]
    enum Msg {
        Start,
        Step(&'static str),
    }

    struct BatchModel {
        trace: Vec<&'static str>,
    }

    impl Model for BatchModel {
        type Msg = Msg;

        fn update(&mut self, msg: Self::Msg, _cx: &mut App) -> Command<Self::Msg> {
            match msg {
                Msg::Start => {
                    self.trace.push("start");
                    Command::batch([
                        Command::none(),
                        Command::emit(Msg::Step("step-1")),
                        Command::batch([Command::none(), Command::emit(Msg::Step("step-2"))]),
                    ])
                }
                Msg::Step(step) => {
                    self.trace.push(step);
                    Command::none()
                }
            }
        }

        fn view(
            &self,
            _window: &mut Window,
            _cx: &mut App,
            _dispatcher: &Dispatcher<Self::Msg>,
        ) -> View {
            div().into_view()
        }
    }

    let program: Entity<Program<BatchModel>> =
        cx.update(|cx| BatchModel { trace: Vec::new() }.into_program(cx));
    let dispatcher = program.read_with(cx, |program, _cx| program.dispatcher());

    dispatcher.dispatch(Msg::Start).unwrap();
    cx.run_until_parked();

    program.read_with(cx, |program, _cx| {
        assert_eq!(program.model().trace, vec!["start", "step-1", "step-2"]);
    });
}

#[gpui::test]
fn non_keyed_effects_and_optional_effects_complete_deterministically(cx: &mut TestAppContext) {
    #[derive(Clone, Debug, PartialEq, Eq)]
    enum Msg {
        RunNext,
        Loaded(&'static str),
    }

    #[derive(Clone, Debug, PartialEq, Eq)]
    struct EffectCompletion {
        label: Option<String>,
        emitted_message: bool,
        message_description: Option<String>,
    }

    struct EffectModel {
        commands: VecDeque<Command<Msg>>,
        completions: Vec<&'static str>,
    }

    impl Model for EffectModel {
        type Msg = Msg;

        fn update(&mut self, msg: Self::Msg, _cx: &mut App) -> Command<Self::Msg> {
            match msg {
                Msg::RunNext => self.commands.pop_front().unwrap_or_else(Command::none),
                Msg::Loaded(label) => {
                    self.completions.push(label);
                    Command::none()
                }
            }
        }

        fn view(
            &self,
            _window: &mut Window,
            _cx: &mut App,
            _dispatcher: &Dispatcher<Self::Msg>,
        ) -> View {
            div().into_view()
        }
    }

    let (foreground_tx, foreground_rx) = oneshot::channel();
    let (background_tx, background_rx) = oneshot::channel();
    let effect_events = Arc::new(Mutex::new(Vec::new()));
    let config = ProgramConfig::default()
        .describe_message(|msg: &Msg| match msg {
            Msg::RunNext => String::from("run-next"),
            Msg::Loaded(label) => format!("loaded:{label}"),
        })
        .observer({
            let effect_events = effect_events.clone();
            move |event: RuntimeEvent<'_, Msg>| {
                if let RuntimeEvent::EffectCompleted {
                    label,
                    emitted_message,
                    message_description,
                    ..
                } = event
                {
                    effect_events.lock().unwrap().push(EffectCompletion {
                        label: label.map(ToOwned::to_owned),
                        emitted_message,
                        message_description: message_description.map(|value| value.to_string()),
                    });
                }
            }
        });

    let commands = VecDeque::from([
        Command::foreground(|_: &mut gpui::AsyncApp| async { None }).label("foreground-none"),
        Command::background(|_| async { None }).label("background-none"),
        Command::foreground(move |_: &mut gpui::AsyncApp| async move {
            foreground_rx.await.ok().map(Msg::Loaded)
        })
        .label("foreground-some"),
        Command::background(move |_| async move { background_rx.await.ok().map(Msg::Loaded) })
            .label("background-some"),
    ]);

    let program: Entity<Program<EffectModel>> = cx.update(|cx| {
        EffectModel {
            commands,
            completions: Vec::new(),
        }
        .into_program_with(config, cx)
    });
    let dispatcher = program.read_with(cx, |program, _cx| program.dispatcher());

    dispatcher.dispatch(Msg::RunNext).unwrap();
    dispatcher.dispatch(Msg::RunNext).unwrap();
    cx.run_until_parked();

    program.read_with(cx, |program, _cx| {
        assert!(program.model().completions.is_empty());
    });

    cx.executor().allow_parking();
    dispatcher.dispatch(Msg::RunNext).unwrap();
    dispatcher.dispatch(Msg::RunNext).unwrap();
    cx.run_until_parked();

    foreground_tx.send("foreground").unwrap();
    cx.run_until_parked();
    program.read_with(cx, |program, _cx| {
        assert_eq!(program.model().completions, vec!["foreground"]);
    });

    background_tx.send("background").unwrap();
    cx.run_until_parked();
    cx.executor().forbid_parking();

    program.read_with(cx, |program, _cx| {
        assert_eq!(
            program.model().completions,
            vec!["foreground", "background"]
        );
    });
    assert_eq!(
        effect_events.lock().unwrap().as_slice(),
        &[
            EffectCompletion {
                label: Some(String::from("foreground-none")),
                emitted_message: false,
                message_description: None,
            },
            EffectCompletion {
                label: Some(String::from("background-none")),
                emitted_message: false,
                message_description: None,
            },
            EffectCompletion {
                label: Some(String::from("foreground-some")),
                emitted_message: true,
                message_description: Some(String::from("loaded:foreground")),
            },
            EffectCompletion {
                label: Some(String::from("background-some")),
                emitted_message: true,
                message_description: Some(String::from("loaded:background")),
            },
        ]
    );
}

#[gpui::test]
fn mapped_command_transforms_messages_and_preserves_keyed_observability(cx: &mut TestAppContext) {
    #[derive(Clone, Debug, PartialEq, Eq)]
    enum Msg {
        RunMapped,
        Loaded(i32),
    }

    #[derive(Clone, Debug, PartialEq, Eq)]
    enum ChildMsg {
        Loaded(i32),
    }

    #[derive(Clone, Debug, PartialEq, Eq)]
    struct CommandScheduledRecord {
        kind: CommandKind,
        label: Option<String>,
        key_description: Option<String>,
    }

    struct MappedCommandModel {
        values: Vec<i32>,
    }

    impl Model for MappedCommandModel {
        type Msg = Msg;

        fn update(&mut self, msg: Self::Msg, _cx: &mut App) -> Command<Self::Msg> {
            match msg {
                Msg::RunMapped => {
                    Command::foreground_keyed("mapped-command", |_: &mut gpui::AsyncApp| async {
                        Some(ChildMsg::Loaded(7))
                    })
                    .label("mapped-command")
                    .map(|msg| match msg {
                        ChildMsg::Loaded(value) => Msg::Loaded(value),
                    })
                }
                Msg::Loaded(value) => {
                    self.values.push(value);
                    Command::none()
                }
            }
        }

        fn view(
            &self,
            _window: &mut Window,
            _cx: &mut App,
            _dispatcher: &Dispatcher<Self::Msg>,
        ) -> View {
            div().into_view()
        }
    }

    let scheduled = Arc::new(Mutex::new(Vec::new()));
    let config = ProgramConfig::default()
        .describe_key(|key| format!("{key:?}"))
        .observer({
            let scheduled = scheduled.clone();
            move |event: RuntimeEvent<'_, Msg>| {
                if let RuntimeEvent::CommandScheduled {
                    kind,
                    label,
                    key_description,
                    ..
                } = event
                {
                    scheduled.lock().unwrap().push(CommandScheduledRecord {
                        kind,
                        label: label.map(ToOwned::to_owned),
                        key_description: key_description.map(|value| value.to_string()),
                    });
                }
            }
        });

    let program: Entity<Program<MappedCommandModel>> =
        cx.update(|cx| MappedCommandModel { values: Vec::new() }.into_program_with(config, cx));
    let dispatcher = program.read_with(cx, |program, _cx| program.dispatcher());

    dispatcher.dispatch(Msg::RunMapped).unwrap();
    cx.run_until_parked();

    program.read_with(cx, |program, _cx| {
        assert_eq!(program.model().values, vec![7]);
    });
    assert_eq!(
        scheduled.lock().unwrap().as_slice(),
        &[CommandScheduledRecord {
            kind: CommandKind::Foreground,
            label: Some(String::from("mapped-command")),
            key_description: Some(String::from("Key(\"mapped-command\")")),
        }]
    );
}

#[gpui::test]
fn keyed_effects_are_latest_wins_and_cancel_replaced_tasks(cx: &mut TestAppContext) {
    #[derive(Clone, Debug, PartialEq, Eq)]
    enum Msg {
        RunNext,
        Loaded(i32),
    }

    #[derive(Clone, Debug, PartialEq, Eq)]
    enum KeyedEvent {
        Replaced {
            key_description: Option<String>,
            previous_label: Option<String>,
            next_label: Option<String>,
        },
        Completed {
            label: Option<String>,
            emitted_message: bool,
            message_description: Option<String>,
        },
    }

    struct KeyedModel {
        commands: VecDeque<Command<Msg>>,
        values: Vec<i32>,
    }

    impl Model for KeyedModel {
        type Msg = Msg;

        fn update(&mut self, msg: Self::Msg, _cx: &mut App) -> Command<Self::Msg> {
            match msg {
                Msg::RunNext => self.commands.pop_front().unwrap_or_else(Command::none),
                Msg::Loaded(value) => {
                    self.values.push(value);
                    Command::none()
                }
            }
        }

        fn view(
            &self,
            _window: &mut Window,
            _cx: &mut App,
            _dispatcher: &Dispatcher<Self::Msg>,
        ) -> View {
            div().into_view()
        }
    }

    let (first_tx, first_rx) = oneshot::channel();
    let (second_tx, second_rx) = oneshot::channel();
    let events = Arc::new(Mutex::new(Vec::new()));
    let config = ProgramConfig::default()
        .describe_message(|msg: &Msg| match msg {
            Msg::RunNext => String::from("run-next"),
            Msg::Loaded(value) => format!("loaded:{value}"),
        })
        .describe_key(|key| format!("{key:?}"))
        .observer({
            let events = events.clone();
            move |event: RuntimeEvent<'_, Msg>| {
                let record = match event {
                    RuntimeEvent::KeyedCommandReplaced {
                        key_description,
                        previous_label,
                        next_label,
                        ..
                    } => Some(KeyedEvent::Replaced {
                        key_description: key_description.map(|value| value.to_string()),
                        previous_label: previous_label.map(ToOwned::to_owned),
                        next_label: next_label.map(ToOwned::to_owned),
                    }),
                    RuntimeEvent::EffectCompleted {
                        label,
                        emitted_message,
                        message_description,
                        ..
                    } => Some(KeyedEvent::Completed {
                        label: label.map(ToOwned::to_owned),
                        emitted_message,
                        message_description: message_description.map(|value| value.to_string()),
                    }),
                    _ => None,
                };

                if let Some(record) = record {
                    events.lock().unwrap().push(record);
                }
            }
        });

    let commands = VecDeque::from([
        Command::background_keyed("load", move |_| async move {
            first_rx.await.ok().map(Msg::Loaded)
        })
        .label("first"),
        Command::background_keyed("load", move |_| async move {
            second_rx.await.ok().map(Msg::Loaded)
        })
        .label("second"),
    ]);

    let program: Entity<Program<KeyedModel>> = cx.update(|cx| {
        KeyedModel {
            commands,
            values: Vec::new(),
        }
        .into_program_with(config, cx)
    });
    let dispatcher = program.read_with(cx, |program, _cx| program.dispatcher());

    cx.executor().allow_parking();
    dispatcher.dispatch(Msg::RunNext).unwrap();
    dispatcher.dispatch(Msg::RunNext).unwrap();
    cx.run_until_parked();

    second_tx.send(2).unwrap();
    cx.run_until_parked();
    program.read_with(cx, |program, _cx| {
        assert_eq!(program.model().values, vec![2]);
    });

    cx.executor().forbid_parking();

    program.read_with(cx, |program, _cx| {
        assert_eq!(program.model().values, vec![2]);
    });
    assert_eq!(first_tx.send(1), Err(1));
    assert_eq!(
        events.lock().unwrap().as_slice(),
        &[
            KeyedEvent::Replaced {
                key_description: Some(String::from("Key(\"load\")")),
                previous_label: Some(String::from("first")),
                next_label: Some(String::from("second")),
            },
            KeyedEvent::Completed {
                label: Some(String::from("second")),
                emitted_message: true,
                message_description: Some(String::from("loaded:2")),
            },
        ]
    );
}

#[gpui::test]
fn subscription_lifecycle_events_include_key_descriptions_and_labels_without_rebuilds(
    cx: &mut TestAppContext,
) {
    #[derive(Clone, Debug, PartialEq, Eq)]
    enum Msg {
        SetSubscription(Option<(&'static str, &'static str)>),
    }

    #[derive(Clone, Debug, PartialEq, Eq)]
    enum SubscriptionEvent {
        Built {
            key_description: Option<String>,
            label: Option<String>,
        },
        Retained {
            key_description: Option<String>,
            label: Option<String>,
        },
        Removed {
            key_description: Option<String>,
            label: Option<String>,
        },
        Summary {
            active: usize,
            added: usize,
            removed: usize,
            retained: usize,
        },
    }

    struct LifecycleModel {
        subscription: Option<(&'static str, &'static str)>,
        builds: Arc<AtomicUsize>,
    }

    impl Model for LifecycleModel {
        type Msg = Msg;

        fn update(&mut self, msg: Self::Msg, _cx: &mut App) -> Command<Self::Msg> {
            match msg {
                Msg::SetSubscription(subscription) => {
                    self.subscription = subscription;
                    Command::none()
                }
            }
        }

        fn subscriptions(&self, _cx: &mut App) -> Subscriptions<Self::Msg> {
            let Some((key, label)) = self.subscription else {
                return Subscriptions::none();
            };

            let builds = self.builds.clone();
            Subscriptions::one(
                Subscription::new(key, move |_cx| {
                    builds.fetch_add(1, Ordering::SeqCst);
                    SubHandle::None
                })
                .label(label),
            )
        }

        fn view(
            &self,
            _window: &mut Window,
            _cx: &mut App,
            _dispatcher: &Dispatcher<Self::Msg>,
        ) -> View {
            div().into_view()
        }
    }

    let events = Arc::new(Mutex::new(Vec::new()));
    let config = ProgramConfig::default()
        .describe_key(|key| format!("{key:?}"))
        .observer({
            let events = events.clone();
            move |event: RuntimeEvent<'_, Msg>| {
                let record = match event {
                    RuntimeEvent::SubscriptionBuilt {
                        key_description,
                        label,
                        ..
                    } => Some(SubscriptionEvent::Built {
                        key_description: key_description.map(|value| value.to_string()),
                        label: label.map(ToOwned::to_owned),
                    }),
                    RuntimeEvent::SubscriptionRetained {
                        key_description,
                        label,
                        ..
                    } => Some(SubscriptionEvent::Retained {
                        key_description: key_description.map(|value| value.to_string()),
                        label: label.map(ToOwned::to_owned),
                    }),
                    RuntimeEvent::SubscriptionRemoved {
                        key_description,
                        label,
                        ..
                    } => Some(SubscriptionEvent::Removed {
                        key_description: key_description.map(|value| value.to_string()),
                        label: label.map(ToOwned::to_owned),
                    }),
                    RuntimeEvent::SubscriptionsReconciled {
                        active,
                        added,
                        removed,
                        retained,
                    } => Some(SubscriptionEvent::Summary {
                        active,
                        added,
                        removed,
                        retained,
                    }),
                    _ => None,
                };

                if let Some(record) = record {
                    events.lock().unwrap().push(record);
                }
            }
        });

    let builds = Arc::new(AtomicUsize::new(0));
    let program: Entity<Program<LifecycleModel>> = cx.update(|cx| {
        LifecycleModel {
            subscription: Some(("alpha", "alpha-v1")),
            builds: builds.clone(),
        }
        .into_program_with(config, cx)
    });
    let dispatcher = program.read_with(cx, |program, _cx| program.dispatcher());

    cx.run_until_parked();
    assert_eq!(builds.load(Ordering::SeqCst), 1);
    assert_eq!(
        events.lock().unwrap().as_slice(),
        &[
            SubscriptionEvent::Built {
                key_description: Some(String::from("Key(\"alpha\")")),
                label: Some(String::from("alpha-v1")),
            },
            SubscriptionEvent::Summary {
                active: 1,
                added: 1,
                removed: 0,
                retained: 0,
            },
        ]
    );

    events.lock().unwrap().clear();

    dispatcher
        .dispatch(Msg::SetSubscription(Some(("alpha", "alpha-v2"))))
        .unwrap();
    cx.run_until_parked();
    assert_eq!(builds.load(Ordering::SeqCst), 1);
    assert_eq!(
        events.lock().unwrap().as_slice(),
        &[
            SubscriptionEvent::Retained {
                key_description: Some(String::from("Key(\"alpha\")")),
                label: Some(String::from("alpha-v2")),
            },
            SubscriptionEvent::Summary {
                active: 1,
                added: 0,
                removed: 0,
                retained: 1,
            },
        ]
    );

    events.lock().unwrap().clear();

    dispatcher
        .dispatch(Msg::SetSubscription(Some(("beta", "beta-v1"))))
        .unwrap();
    cx.run_until_parked();
    assert_eq!(builds.load(Ordering::SeqCst), 2);
    assert_eq!(
        events.lock().unwrap().as_slice(),
        &[
            SubscriptionEvent::Built {
                key_description: Some(String::from("Key(\"beta\")")),
                label: Some(String::from("beta-v1")),
            },
            SubscriptionEvent::Removed {
                key_description: Some(String::from("Key(\"alpha\")")),
                label: Some(String::from("alpha-v2")),
            },
            SubscriptionEvent::Summary {
                active: 1,
                added: 1,
                removed: 1,
                retained: 0,
            },
        ]
    );

    events.lock().unwrap().clear();

    dispatcher.dispatch(Msg::SetSubscription(None)).unwrap();
    cx.run_until_parked();
    assert_eq!(builds.load(Ordering::SeqCst), 2);
    assert_eq!(
        events.lock().unwrap().as_slice(),
        &[
            SubscriptionEvent::Removed {
                key_description: Some(String::from("Key(\"beta\")")),
                label: Some(String::from("beta-v1")),
            },
            SubscriptionEvent::Summary {
                active: 0,
                added: 0,
                removed: 1,
                retained: 0,
            },
        ]
    );
}

#[gpui::test]
fn subscriptions_are_retained_rebuilt_removed_and_can_dispatch_messages(cx: &mut TestAppContext) {
    #[derive(Clone, Debug, PartialEq, Eq)]
    enum Msg {
        SetKey(Option<&'static str>),
        FromSubscription(&'static str),
    }

    #[derive(Clone, Debug, PartialEq, Eq)]
    enum ChildMsg {
        Fire(&'static str),
    }

    #[derive(Clone, Debug, PartialEq, Eq)]
    struct ReconcileStats {
        active: usize,
        added: usize,
        removed: usize,
        retained: usize,
    }

    struct SubscriptionModel {
        key: Option<&'static str>,
        delivered: Vec<&'static str>,
        builds: Arc<Mutex<Vec<&'static str>>>,
    }

    impl Model for SubscriptionModel {
        type Msg = Msg;

        fn update(&mut self, msg: Self::Msg, _cx: &mut App) -> Command<Self::Msg> {
            match msg {
                Msg::SetKey(key) => {
                    self.key = key;
                    Command::none()
                }
                Msg::FromSubscription(value) => {
                    self.delivered.push(value);
                    Command::none()
                }
            }
        }

        fn subscriptions(&self, _cx: &mut App) -> Subscriptions<Self::Msg> {
            let Some(key) = self.key else {
                return Subscriptions::none();
            };

            let builds = self.builds.clone();
            let inner = Subscription::new(key, move |cx| {
                builds.lock().unwrap().push(key);
                cx.dispatch(ChildMsg::Fire(key)).unwrap();
                SubHandle::None
            })
            .label("mapped-subscription");

            Subscriptions::one(inner.map(|msg| match msg {
                ChildMsg::Fire(value) => Msg::FromSubscription(value),
            }))
        }

        fn view(
            &self,
            _window: &mut Window,
            _cx: &mut App,
            _dispatcher: &Dispatcher<Self::Msg>,
        ) -> View {
            div().into_view()
        }
    }

    let reconcile_stats = Arc::new(Mutex::new(Vec::new()));
    let config = ProgramConfig::default().observer({
        let reconcile_stats = reconcile_stats.clone();
        move |event: RuntimeEvent<'_, Msg>| {
            if let RuntimeEvent::SubscriptionsReconciled {
                active,
                added,
                removed,
                retained,
            } = event
            {
                reconcile_stats.lock().unwrap().push(ReconcileStats {
                    active,
                    added,
                    removed,
                    retained,
                });
            }
        }
    });

    let builds = Arc::new(Mutex::new(Vec::new()));
    let program: Entity<Program<SubscriptionModel>> = cx.update(|cx| {
        SubscriptionModel {
            key: Some("alpha"),
            delivered: Vec::new(),
            builds: builds.clone(),
        }
        .into_program_with(config, cx)
    });
    let dispatcher = program.read_with(cx, |program, _cx| program.dispatcher());

    cx.run_until_parked();
    program.read_with(cx, |program, _cx| {
        assert_eq!(program.model().delivered, vec!["alpha"]);
    });
    assert_eq!(builds.lock().unwrap().as_slice(), &["alpha"]);

    reconcile_stats.lock().unwrap().clear();

    dispatcher.dispatch(Msg::SetKey(Some("alpha"))).unwrap();
    cx.run_until_parked();
    program.read_with(cx, |program, _cx| {
        assert_eq!(program.model().delivered, vec!["alpha"]);
    });
    assert_eq!(builds.lock().unwrap().as_slice(), &["alpha"]);
    assert_eq!(
        reconcile_stats.lock().unwrap().as_slice(),
        &[ReconcileStats {
            active: 1,
            added: 0,
            removed: 0,
            retained: 1,
        }]
    );

    reconcile_stats.lock().unwrap().clear();

    dispatcher.dispatch(Msg::SetKey(Some("beta"))).unwrap();
    cx.run_until_parked();
    program.read_with(cx, |program, _cx| {
        assert_eq!(program.model().delivered, vec!["alpha", "beta"]);
    });
    assert_eq!(builds.lock().unwrap().as_slice(), &["alpha", "beta"]);
    let beta_reconciles = reconcile_stats.lock().unwrap().clone();
    assert!(beta_reconciles.contains(&ReconcileStats {
        active: 1,
        added: 1,
        removed: 1,
        retained: 0,
    }));
    assert!(beta_reconciles.contains(&ReconcileStats {
        active: 1,
        added: 0,
        removed: 0,
        retained: 1,
    }));

    reconcile_stats.lock().unwrap().clear();

    dispatcher.dispatch(Msg::SetKey(None)).unwrap();
    cx.run_until_parked();
    assert_eq!(
        reconcile_stats.lock().unwrap().as_slice(),
        &[ReconcileStats {
            active: 0,
            added: 0,
            removed: 1,
            retained: 0,
        }]
    );
}

#[gpui::test]
fn nested_child_init_and_update_lift_messages_through_scope(cx: &mut TestAppContext) {
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum ChildMsg {
        Ready,
        Increment,
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum Msg {
        Left(ChildMsg),
        Right(ChildMsg),
    }

    struct CounterChild {
        init_calls: usize,
        value: i32,
    }

    impl NestedModel for CounterChild {
        type Msg = ChildMsg;

        fn init(&mut self, _cx: &mut App, _scope: &ModelContext<Self::Msg>) -> Command<Self::Msg> {
            self.init_calls += 1;
            Command::emit(ChildMsg::Ready)
        }

        fn update(
            &mut self,
            msg: Self::Msg,
            _cx: &mut App,
            _scope: &ModelContext<Self::Msg>,
        ) -> Command<Self::Msg> {
            match msg {
                ChildMsg::Ready => Command::none(),
                ChildMsg::Increment => {
                    self.value += 1;
                    Command::none()
                }
            }
        }

        fn view(
            &self,
            _window: &mut Window,
            _cx: &mut App,
            _scope: &ModelContext<Self::Msg>,
            _dispatcher: &Dispatcher<Self::Msg>,
        ) -> View {
            div().into_view()
        }
    }

    struct ParentModel {
        left: CounterChild,
        right: CounterChild,
        ready: Vec<&'static str>,
    }

    impl NestedModel for ParentModel {
        type Msg = Msg;

        fn init(&mut self, cx: &mut App, scope: &ModelContext<Self::Msg>) -> Command<Self::Msg> {
            let left = scope.scope("left", Msg::Left);
            let right = scope.scope("right", Msg::Right);

            Command::batch([
                left.init(&mut self.left, cx),
                right.init(&mut self.right, cx),
            ])
        }

        fn update(
            &mut self,
            msg: Self::Msg,
            cx: &mut App,
            scope: &ModelContext<Self::Msg>,
        ) -> Command<Self::Msg> {
            match msg {
                Msg::Left(ChildMsg::Ready) => {
                    self.ready.push("left");
                    Command::none()
                }
                Msg::Right(ChildMsg::Ready) => {
                    self.ready.push("right");
                    Command::none()
                }
                Msg::Left(child_msg) => {
                    scope
                        .scope("left", Msg::Left)
                        .update(&mut self.left, child_msg, cx)
                }
                Msg::Right(child_msg) => {
                    scope
                        .scope("right", Msg::Right)
                        .update(&mut self.right, child_msg, cx)
                }
            }
        }

        fn view(
            &self,
            _window: &mut Window,
            _cx: &mut App,
            _scope: &ModelContext<Self::Msg>,
            _dispatcher: &Dispatcher<Self::Msg>,
        ) -> View {
            div().into_view()
        }
    }

    let program: Entity<Program<ParentModel>> = cx.update(|cx| {
        ParentModel {
            left: CounterChild {
                init_calls: 0,
                value: 0,
            },
            right: CounterChild {
                init_calls: 0,
                value: 0,
            },
            ready: Vec::new(),
        }
        .into_program(cx)
    });
    let dispatcher = program.read_with(cx, |program, _cx| program.dispatcher());

    cx.run_until_parked();
    dispatcher.dispatch(Msg::Left(ChildMsg::Increment)).unwrap();
    dispatcher
        .dispatch(Msg::Right(ChildMsg::Increment))
        .unwrap();
    cx.run_until_parked();

    program.read_with(cx, |program, _cx| {
        assert_eq!(program.model().ready, vec!["left", "right"]);
        assert_eq!(program.model().left.init_calls, 1);
        assert_eq!(program.model().right.init_calls, 1);
        assert_eq!(program.model().left.value, 1);
        assert_eq!(program.model().right.value, 1);
    });
}

#[gpui::test]
fn sibling_nested_keyed_commands_keep_distinct_paths(cx: &mut TestAppContext) {
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum ChildMsg {
        Run,
        Loaded(i32),
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum Msg {
        Left(ChildMsg),
        Right(ChildMsg),
    }

    #[derive(Clone, Debug, PartialEq, Eq)]
    struct KeyRecord {
        path: Option<ChildPath>,
        local_id: Option<String>,
    }

    struct LoaderChild {
        value: Option<i32>,
    }

    impl NestedModel for LoaderChild {
        type Msg = ChildMsg;

        fn update(
            &mut self,
            msg: Self::Msg,
            _cx: &mut App,
            _scope: &ModelContext<Self::Msg>,
        ) -> Command<Self::Msg> {
            match msg {
                ChildMsg::Run => {
                    Command::foreground_keyed("load", |_: &mut gpui::AsyncApp| async {
                        Some(ChildMsg::Loaded(7))
                    })
                }
                ChildMsg::Loaded(value) => {
                    self.value = Some(value);
                    Command::none()
                }
            }
        }

        fn view(
            &self,
            _window: &mut Window,
            _cx: &mut App,
            _scope: &ModelContext<Self::Msg>,
            _dispatcher: &Dispatcher<Self::Msg>,
        ) -> View {
            div().into_view()
        }
    }

    struct ParentModel {
        left: LoaderChild,
        right: LoaderChild,
    }

    impl NestedModel for ParentModel {
        type Msg = Msg;

        fn update(
            &mut self,
            msg: Self::Msg,
            cx: &mut App,
            scope: &ModelContext<Self::Msg>,
        ) -> Command<Self::Msg> {
            match msg {
                Msg::Left(child_msg) => {
                    scope
                        .scope("left", Msg::Left)
                        .update(&mut self.left, child_msg, cx)
                }
                Msg::Right(child_msg) => {
                    scope
                        .scope("right", Msg::Right)
                        .update(&mut self.right, child_msg, cx)
                }
            }
        }

        fn view(
            &self,
            _window: &mut Window,
            _cx: &mut App,
            _scope: &ModelContext<Self::Msg>,
            _dispatcher: &Dispatcher<Self::Msg>,
        ) -> View {
            div().into_view()
        }
    }

    let scheduled = Arc::new(Mutex::new(Vec::new()));
    let config = ProgramConfig::default().observer({
        let scheduled = scheduled.clone();
        move |event: RuntimeEvent<'_, Msg>| {
            if let RuntimeEvent::CommandScheduled { key: Some(key), .. } = event {
                scheduled.lock().unwrap().push(KeyRecord {
                    path: key.child_path().cloned(),
                    local_id: key.local_id().map(ToOwned::to_owned),
                });
            }
        }
    });

    let program: Entity<Program<ParentModel>> = cx.update(|cx| {
        ParentModel {
            left: LoaderChild { value: None },
            right: LoaderChild { value: None },
        }
        .into_program_with(config, cx)
    });
    let dispatcher = program.read_with(cx, |program, _cx| program.dispatcher());

    dispatcher.dispatch(Msg::Left(ChildMsg::Run)).unwrap();
    dispatcher.dispatch(Msg::Right(ChildMsg::Run)).unwrap();
    cx.run_until_parked();

    program.read_with(cx, |program, _cx| {
        assert_eq!(program.model().left.value, Some(7));
        assert_eq!(program.model().right.value, Some(7));
    });
    assert_eq!(
        scheduled.lock().unwrap().as_slice(),
        &[
            KeyRecord {
                path: Some(ChildPath::new("left")),
                local_id: Some(String::from("load")),
            },
            KeyRecord {
                path: Some(ChildPath::new("right")),
                local_id: Some(String::from("load")),
            },
        ]
    );
}

#[gpui::test]
fn sibling_nested_subscriptions_do_not_conflict_on_local_keys(cx: &mut TestAppContext) {
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum ChildMsg {
        FromSubscription(&'static str),
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum Msg {
        Left(ChildMsg),
        Right(ChildMsg),
    }

    struct SubscriptionChild {
        id: &'static str,
    }

    impl NestedModel for SubscriptionChild {
        type Msg = ChildMsg;

        fn update(
            &mut self,
            _msg: Self::Msg,
            _cx: &mut App,
            _scope: &ModelContext<Self::Msg>,
        ) -> Command<Self::Msg> {
            Command::none()
        }

        fn subscriptions(
            &self,
            _cx: &mut App,
            _scope: &ModelContext<Self::Msg>,
        ) -> Subscriptions<Self::Msg> {
            let id = self.id;
            Subscriptions::one(Subscription::new("clock", move |cx| {
                cx.dispatch(ChildMsg::FromSubscription(id)).unwrap();
                SubHandle::None
            }))
        }

        fn view(
            &self,
            _window: &mut Window,
            _cx: &mut App,
            _scope: &ModelContext<Self::Msg>,
            _dispatcher: &Dispatcher<Self::Msg>,
        ) -> View {
            div().into_view()
        }
    }

    struct ParentModel {
        left: SubscriptionChild,
        right: SubscriptionChild,
        delivered: Vec<&'static str>,
    }

    impl NestedModel for ParentModel {
        type Msg = Msg;

        fn update(
            &mut self,
            msg: Self::Msg,
            _cx: &mut App,
            _scope: &ModelContext<Self::Msg>,
        ) -> Command<Self::Msg> {
            match msg {
                Msg::Left(ChildMsg::FromSubscription(value))
                | Msg::Right(ChildMsg::FromSubscription(value)) => self.delivered.push(value),
            }

            Command::none()
        }

        fn subscriptions(
            &self,
            cx: &mut App,
            scope: &ModelContext<Self::Msg>,
        ) -> Subscriptions<Self::Msg> {
            let left = scope.scope("left", Msg::Left);
            let right = scope.scope("right", Msg::Right);
            let mut subscriptions = Subscriptions::none();
            for subscription in left.subscriptions(&self.left, cx) {
                subscriptions
                    .push(subscription)
                    .expect("left child subscriptions should keep unique keys");
            }
            for subscription in right.subscriptions(&self.right, cx) {
                subscriptions
                    .push(subscription)
                    .expect("right child subscriptions should keep unique keys");
            }
            subscriptions
        }

        fn view(
            &self,
            _window: &mut Window,
            _cx: &mut App,
            _scope: &ModelContext<Self::Msg>,
            _dispatcher: &Dispatcher<Self::Msg>,
        ) -> View {
            div().into_view()
        }
    }

    let program: Entity<Program<ParentModel>> = cx.update(|cx| {
        ParentModel {
            left: SubscriptionChild { id: "left" },
            right: SubscriptionChild { id: "right" },
            delivered: Vec::new(),
        }
        .into_program(cx)
    });

    cx.run_until_parked();

    program.read_with(cx, |program, _cx| {
        assert_eq!(program.model().delivered, vec!["left", "right"]);
    });
}

#[gpui::test]
fn deep_nested_scope_rebases_grandchild_keys(cx: &mut TestAppContext) {
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum GrandChildMsg {
        Run,
        Loaded,
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum ChildMsg {
        RunGrandchild,
        Grandchild(GrandChildMsg),
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum Msg {
        Child(ChildMsg),
    }

    struct Grandchild {
        completed: bool,
    }

    impl NestedModel for Grandchild {
        type Msg = GrandChildMsg;

        fn update(
            &mut self,
            msg: Self::Msg,
            _cx: &mut App,
            _scope: &ModelContext<Self::Msg>,
        ) -> Command<Self::Msg> {
            match msg {
                GrandChildMsg::Run => {
                    Command::foreground_keyed("load", |_: &mut gpui::AsyncApp| async {
                        Some(GrandChildMsg::Loaded)
                    })
                }
                GrandChildMsg::Loaded => {
                    self.completed = true;
                    Command::none()
                }
            }
        }

        fn view(
            &self,
            _window: &mut Window,
            _cx: &mut App,
            _scope: &ModelContext<Self::Msg>,
            _dispatcher: &Dispatcher<Self::Msg>,
        ) -> View {
            div().into_view()
        }
    }

    struct ChildModel {
        grandchild: Grandchild,
    }

    impl NestedModel for ChildModel {
        type Msg = ChildMsg;

        fn update(
            &mut self,
            msg: Self::Msg,
            cx: &mut App,
            scope: &ModelContext<Self::Msg>,
        ) -> Command<Self::Msg> {
            let grandchild = scope.scope("grandchild", ChildMsg::Grandchild);
            match msg {
                ChildMsg::RunGrandchild => {
                    grandchild.update(&mut self.grandchild, GrandChildMsg::Run, cx)
                }
                ChildMsg::Grandchild(grandchild_msg) => {
                    grandchild.update(&mut self.grandchild, grandchild_msg, cx)
                }
            }
        }

        fn view(
            &self,
            _window: &mut Window,
            _cx: &mut App,
            _scope: &ModelContext<Self::Msg>,
            _dispatcher: &Dispatcher<Self::Msg>,
        ) -> View {
            div().into_view()
        }
    }

    struct ParentModel {
        child: ChildModel,
    }

    impl NestedModel for ParentModel {
        type Msg = Msg;

        fn update(
            &mut self,
            msg: Self::Msg,
            cx: &mut App,
            scope: &ModelContext<Self::Msg>,
        ) -> Command<Self::Msg> {
            match msg {
                Msg::Child(child_msg) => {
                    scope
                        .scope("child", Msg::Child)
                        .update(&mut self.child, child_msg, cx)
                }
            }
        }

        fn view(
            &self,
            _window: &mut Window,
            _cx: &mut App,
            _scope: &ModelContext<Self::Msg>,
            _dispatcher: &Dispatcher<Self::Msg>,
        ) -> View {
            div().into_view()
        }
    }

    let observed_keys = Arc::new(Mutex::new(Vec::new()));
    let config = ProgramConfig::default().observer({
        let observed_keys = observed_keys.clone();
        move |event: RuntimeEvent<'_, Msg>| {
            if let RuntimeEvent::EffectCompleted { key: Some(key), .. } = event {
                observed_keys.lock().unwrap().push((
                    key.child_path().cloned(),
                    key.local_id().map(ToOwned::to_owned),
                ));
            }
        }
    });

    let program: Entity<Program<ParentModel>> = cx.update(|cx| {
        ParentModel {
            child: ChildModel {
                grandchild: Grandchild { completed: false },
            },
        }
        .into_program_with(config, cx)
    });
    let dispatcher = program.read_with(cx, |program, _cx| program.dispatcher());

    dispatcher
        .dispatch(Msg::Child(ChildMsg::RunGrandchild))
        .unwrap();
    cx.run_until_parked();

    program.read_with(cx, |program, _cx| {
        assert!(program.model().child.grandchild.completed);
    });
    assert_eq!(
        observed_keys.lock().unwrap().as_slice(),
        &[(
            Some(ChildPath::new("child").child("grandchild")),
            Some(String::from("load"))
        )]
    );
}
