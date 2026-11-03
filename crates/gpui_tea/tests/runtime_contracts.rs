//! Test or Example
use futures::channel::oneshot;
use gpui::{App, Entity, TestAppContext, Window, div};
use gpui_tea::{
    ChildPath, Command, CommandKind, Composite, Dispatcher, IntoView, Model, ModelContext,
    ModelExt, Program, ProgramConfig, QueueOverflowAction, QueuePolicy, RuntimeEvent, SubHandle,
    Subscription, Subscriptions, TelemetryEnvelope, TelemetryEvent, View,
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

        fn init(&mut self, _cx: &mut App, _scope: &ModelContext<Self::Msg>) -> Command<Self::Msg> {
            self.init_calls += 1;
            Command::emit(Msg::InitLoaded(41)).label("init")
        }

        fn update(
            &mut self,
            msg: Self::Msg,
            _cx: &mut App,
            _scope: &ModelContext<Self::Msg>,
        ) -> Command<Self::Msg> {
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

        fn subscriptions(
            &self,
            _cx: &mut App,
            _scope: &ModelContext<Self::Msg>,
        ) -> Subscriptions<Self::Msg> {
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
            _scope: &ModelContext<Self::Msg>,
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

    program.read_with(cx, |program, _cx| {
        assert_eq!(program.model().init_calls, 1);
        assert_eq!(program.model().state, 41);
    });
    assert_eq!(subscription_builds.load(Ordering::SeqCst), 1);

    cx.run_until_parked();

    program.read_with(cx, |program, _cx| {
        assert_eq!(program.model().init_calls, 1);
        assert_eq!(program.model().state, 41);
        assert_eq!(program.model().delivered, vec!["ready"]);
    });
    assert_eq!(subscription_builds.load(Ordering::SeqCst), 1);
}

#[gpui::test]
fn mount_drains_batched_init_emits_with_update_equivalent_fifo_ordering(cx: &mut TestAppContext) {
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum Msg {
        A,
        B,
        C,
    }

    struct InitBatchModel {
        trace: Vec<&'static str>,
    }

    impl Model for InitBatchModel {
        type Msg = Msg;

        fn init(&mut self, _cx: &mut App, _scope: &ModelContext<Self::Msg>) -> Command<Self::Msg> {
            Command::batch([Command::emit(Msg::A), Command::emit(Msg::B)])
        }

        fn update(
            &mut self,
            msg: Self::Msg,
            _cx: &mut App,
            _scope: &ModelContext<Self::Msg>,
        ) -> Command<Self::Msg> {
            match msg {
                Msg::A => {
                    self.trace.push("a");
                    Command::emit(Msg::C)
                }
                Msg::B => {
                    self.trace.push("b");
                    Command::none()
                }
                Msg::C => {
                    self.trace.push("c");
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

    let program: Entity<Program<InitBatchModel>> =
        cx.update(|cx| InitBatchModel { trace: Vec::new() }.into_program(cx));

    program.read_with(cx, |program, _cx| {
        assert_eq!(program.model().trace, vec!["a", "b", "c"]);
    });
}

#[gpui::test]
fn mount_reconciles_initial_subscriptions_after_init_without_enqueued_messages(
    cx: &mut TestAppContext,
) {
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum Msg {}

    struct InitSubscriptionModel {
        initialized: bool,
        init_calls: usize,
        subscription_builds: Arc<AtomicUsize>,
    }

    impl Model for InitSubscriptionModel {
        type Msg = Msg;

        fn init(&mut self, _cx: &mut App, _scope: &ModelContext<Self::Msg>) -> Command<Self::Msg> {
            self.initialized = true;
            self.init_calls += 1;
            Command::none()
        }

        fn update(
            &mut self,
            msg: Self::Msg,
            _cx: &mut App,
            _scope: &ModelContext<Self::Msg>,
        ) -> Command<Self::Msg> {
            match msg {}
        }

        fn subscriptions(
            &self,
            _cx: &mut App,
            _scope: &ModelContext<Self::Msg>,
        ) -> Subscriptions<Self::Msg> {
            if !self.initialized {
                return Subscriptions::none();
            }

            let subscription_builds = self.subscription_builds.clone();
            Subscriptions::one(Subscription::new("init-only", move |_cx| {
                subscription_builds.fetch_add(1, Ordering::SeqCst);
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

    let subscription_builds = Arc::new(AtomicUsize::new(0));
    let program: Entity<Program<InitSubscriptionModel>> = cx.update(|cx| {
        InitSubscriptionModel {
            initialized: false,
            init_calls: 0,
            subscription_builds: subscription_builds.clone(),
        }
        .into_program(cx)
    });

    program.read_with(cx, |program, _cx| {
        assert!(program.model().initialized);
        assert_eq!(program.model().init_calls, 1);
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

        fn update(
            &mut self,
            msg: Self::Msg,
            _cx: &mut App,
            _scope: &ModelContext<Self::Msg>,
        ) -> Command<Self::Msg> {
            match msg {
                Msg::Push(entry) => self.trace.push(entry),
            }

            Command::none()
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

        fn update(
            &mut self,
            msg: Self::Msg,
            _cx: &mut App,
            _scope: &ModelContext<Self::Msg>,
        ) -> Command<Self::Msg> {
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
            _scope: &ModelContext<Self::Msg>,
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

        fn update(
            &mut self,
            msg: Self::Msg,
            _cx: &mut App,
            _scope: &ModelContext<Self::Msg>,
        ) -> Command<Self::Msg> {
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
            _scope: &ModelContext<Self::Msg>,
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

        fn update(
            &mut self,
            msg: Self::Msg,
            _cx: &mut App,
            _scope: &ModelContext<Self::Msg>,
        ) -> Command<Self::Msg> {
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
            _scope: &ModelContext<Self::Msg>,
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

        fn update(
            &mut self,
            msg: Self::Msg,
            _cx: &mut App,
            _scope: &ModelContext<Self::Msg>,
        ) -> Command<Self::Msg> {
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
            _scope: &ModelContext<Self::Msg>,
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
fn non_keyed_effects_are_canceled_when_program_is_dropped(cx: &mut TestAppContext) {
    #[derive(Clone, Debug, PartialEq, Eq)]
    enum Msg {
        Run,
        Loaded(i32),
    }

    struct DropModel {
        receiver: Option<oneshot::Receiver<i32>>,
        loaded: Vec<i32>,
    }

    impl Model for DropModel {
        type Msg = Msg;

        fn update(
            &mut self,
            msg: Self::Msg,
            _cx: &mut App,
            _scope: &ModelContext<Self::Msg>,
        ) -> Command<Self::Msg> {
            match msg {
                Msg::Run => {
                    let receiver = self
                        .receiver
                        .take()
                        .expect("run should only be dispatched once in this test");
                    Command::background(
                        move |_| async move { receiver.await.ok().map(Msg::Loaded) },
                    )
                }
                Msg::Loaded(value) => {
                    self.loaded.push(value);
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

    let (sender, receiver) = oneshot::channel();
    type DropProgram = Entity<Program<DropModel>>;

    let program_slot: Arc<Mutex<Option<DropProgram>>> = Arc::new(Mutex::new(None));
    let window = cx.update(|cx| {
        let program_slot = program_slot.clone();
        cx.open_window(gpui::WindowOptions::default(), move |_, cx| {
            let program = Program::mount(
                DropModel {
                    receiver: Some(receiver),
                    loaded: Vec::new(),
                },
                cx,
            );
            *program_slot.lock().unwrap() = Some(program.clone());
            program
        })
        .unwrap()
    });

    let program = program_slot.lock().unwrap().take().unwrap();
    let dispatcher = program.read_with(cx, |program, _cx| program.dispatcher());
    drop(program);

    cx.executor().allow_parking();
    dispatcher.dispatch(Msg::Run).unwrap();
    cx.run_until_parked();

    cx.update(|cx| {
        window
            .update(cx, |_, window, _| window.remove_window())
            .unwrap();
    });
    cx.run_until_parked();
    cx.executor().forbid_parking();

    assert_eq!(sender.send(7), Err(7));
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

        fn update(
            &mut self,
            msg: Self::Msg,
            _cx: &mut App,
            _scope: &ModelContext<Self::Msg>,
        ) -> Command<Self::Msg> {
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
            _scope: &ModelContext<Self::Msg>,
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
fn child_scopes_preserve_emit_labels_when_lifting_messages(cx: &mut TestAppContext) {
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum ChildMsg {
        Run,
        Loaded,
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum Msg {
        Child(ChildMsg),
    }

    impl Msg {
        fn into_child(self) -> Result<ChildMsg, Self> {
            match self {
                Self::Child(msg) => Ok(msg),
            }
        }
    }

    struct ChildModel;

    impl Model for ChildModel {
        type Msg = ChildMsg;

        fn update(
            &mut self,
            msg: Self::Msg,
            _cx: &mut App,
            _scope: &ModelContext<Self::Msg>,
        ) -> Command<Self::Msg> {
            match msg {
                ChildMsg::Run => Command::emit(ChildMsg::Loaded).label("child-emit"),
                ChildMsg::Loaded => Command::none(),
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

    #[derive(Composite)]
    #[composite(message = Msg)]
    struct ParentModel {
        #[child(path = "child", lift = Msg::Child, extract = Msg::into_child)]
        child: ChildModel,
        loaded: usize,
    }

    impl Model for ParentModel {
        type Msg = Msg;

        fn update(
            &mut self,
            msg: Self::Msg,
            cx: &mut App,
            scope: &ModelContext<Self::Msg>,
        ) -> Command<Self::Msg> {
            match msg {
                Msg::Child(ChildMsg::Loaded) => {
                    self.loaded += 1;
                    Command::none()
                }
                other => match self.__composite_update(other, cx, scope) {
                    Ok(command) => command,
                    Err(other) => panic!("unexpected unmatched message: {other:?}"),
                },
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

    let labels = Arc::new(Mutex::new(Vec::new()));
    let config = ProgramConfig::default().observer({
        let labels = labels.clone();
        move |event: RuntimeEvent<'_, Msg>| {
            if let RuntimeEvent::CommandScheduled { label, .. } = event {
                labels.lock().unwrap().push(label.map(ToOwned::to_owned));
            }
        }
    });

    let program: Entity<Program<ParentModel>> = cx.update(|cx| {
        ParentModel {
            child: ChildModel,
            loaded: 0,
        }
        .into_program_with(config, cx)
    });
    let dispatcher = program.read_with(cx, |program, _cx| program.dispatcher());

    dispatcher.dispatch(Msg::Child(ChildMsg::Run)).unwrap();
    cx.run_until_parked();

    program.read_with(cx, |program, _cx| {
        assert_eq!(program.model().loaded, 1);
    });
    assert_eq!(
        labels.lock().unwrap().as_slice(),
        &[Some(String::from("child-emit"))]
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

        fn update(
            &mut self,
            msg: Self::Msg,
            _cx: &mut App,
            _scope: &ModelContext<Self::Msg>,
        ) -> Command<Self::Msg> {
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
            _scope: &ModelContext<Self::Msg>,
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

    assert_eq!(first_tx.send(1), Err(1));
    cx.run_until_parked();
    cx.executor().forbid_parking();

    program.read_with(cx, |program, _cx| {
        assert_eq!(program.model().values, vec![2]);
    });
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

        fn update(
            &mut self,
            msg: Self::Msg,
            _cx: &mut App,
            _scope: &ModelContext<Self::Msg>,
        ) -> Command<Self::Msg> {
            match msg {
                Msg::SetSubscription(subscription) => {
                    self.subscription = subscription;
                    Command::none()
                }
            }
        }

        fn subscriptions(
            &self,
            _cx: &mut App,
            _scope: &ModelContext<Self::Msg>,
        ) -> Subscriptions<Self::Msg> {
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
            _scope: &ModelContext<Self::Msg>,
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
            SubscriptionEvent::Removed {
                key_description: Some(String::from("Key(\"alpha\")")),
                label: Some(String::from("alpha-v2")),
            },
            SubscriptionEvent::Built {
                key_description: Some(String::from("Key(\"beta\")")),
                label: Some(String::from("beta-v1")),
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

        fn update(
            &mut self,
            msg: Self::Msg,
            _cx: &mut App,
            _scope: &ModelContext<Self::Msg>,
        ) -> Command<Self::Msg> {
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

        fn subscriptions(
            &self,
            _cx: &mut App,
            _scope: &ModelContext<Self::Msg>,
        ) -> Subscriptions<Self::Msg> {
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
            _scope: &ModelContext<Self::Msg>,
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
fn composite_init_batches_child_lifecycle_and_parent_can_intercept_child_messages(
    cx: &mut TestAppContext,
) {
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum ChildMsg {
        Ready,
        Increment,
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum Msg {
        Left(ChildMsg),
        Right(ChildMsg),
        ParentOnly,
    }

    impl Msg {
        fn into_left(self) -> Result<ChildMsg, Self> {
            match self {
                Self::Left(msg) => Ok(msg),
                other => Err(other),
            }
        }

        fn into_right(self) -> Result<ChildMsg, Self> {
            match self {
                Self::Right(msg) => Ok(msg),
                other => Err(other),
            }
        }
    }

    struct CounterChild {
        init_calls: usize,
        value: i32,
    }

    impl Model for CounterChild {
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

    #[derive(Composite)]
    #[composite(message = Msg)]
    struct ParentModel {
        #[child(path = "left", lift = Msg::Left, extract = Msg::into_left)]
        left: CounterChild,
        #[child(path = "right", lift = Msg::Right, extract = Msg::into_right)]
        right: CounterChild,
        ready: Vec<&'static str>,
        parent_only_hits: usize,
    }

    impl Model for ParentModel {
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
            match msg {
                Msg::Left(ChildMsg::Ready) => {
                    self.ready.push("left");
                    Command::none()
                }
                Msg::Right(ChildMsg::Ready) => {
                    self.ready.push("right");
                    Command::none()
                }
                Msg::ParentOnly => {
                    self.parent_only_hits += 1;
                    Command::none()
                }
                other => match self.__composite_update(other, cx, scope) {
                    Ok(command) => command,
                    Err(msg) => panic!("unexpected unmatched message: {msg:?}"),
                },
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
            parent_only_hits: 0,
        }
        .into_program(cx)
    });
    let dispatcher = program.read_with(cx, |program, _cx| program.dispatcher());

    cx.run_until_parked();
    dispatcher.dispatch(Msg::Left(ChildMsg::Increment)).unwrap();
    dispatcher
        .dispatch(Msg::Right(ChildMsg::Increment))
        .unwrap();
    dispatcher.dispatch(Msg::ParentOnly).unwrap();
    cx.run_until_parked();

    program.read_with(cx, |program, _cx| {
        assert_eq!(program.model().ready, vec!["left", "right"]);
        assert_eq!(program.model().left.init_calls, 1);
        assert_eq!(program.model().right.init_calls, 1);
        assert_eq!(program.model().left.value, 1);
        assert_eq!(program.model().right.value, 1);
        assert_eq!(program.model().parent_only_hits, 1);
    });
}

#[gpui::test]
fn composite_init_preserves_sibling_ready_order_before_parent_follow_up(cx: &mut TestAppContext) {
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum ChildMsg {
        Ready,
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum Msg {
        Left(ChildMsg),
        Right(ChildMsg),
        ParentFollowUp,
    }

    impl Msg {
        fn into_left(self) -> Result<ChildMsg, Self> {
            match self {
                Self::Left(msg) => Ok(msg),
                other => Err(other),
            }
        }

        fn into_right(self) -> Result<ChildMsg, Self> {
            match self {
                Self::Right(msg) => Ok(msg),
                other => Err(other),
            }
        }
    }

    struct ReadyChild;

    impl Model for ReadyChild {
        type Msg = ChildMsg;

        fn init(&mut self, _cx: &mut App, _scope: &ModelContext<Self::Msg>) -> Command<Self::Msg> {
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

    #[derive(Composite)]
    #[composite(message = Msg)]
    struct ParentModel {
        #[child(path = "left", lift = Msg::Left, extract = Msg::into_left)]
        left: ReadyChild,
        #[child(path = "right", lift = Msg::Right, extract = Msg::into_right)]
        right: ReadyChild,
        trace: Vec<&'static str>,
    }

    impl Model for ParentModel {
        type Msg = Msg;

        fn init(&mut self, cx: &mut App, scope: &ModelContext<Self::Msg>) -> Command<Self::Msg> {
            self.__composite_init(cx, scope)
        }

        fn update(
            &mut self,
            msg: Self::Msg,
            _cx: &mut App,
            _scope: &ModelContext<Self::Msg>,
        ) -> Command<Self::Msg> {
            match msg {
                Msg::Left(ChildMsg::Ready) => {
                    self.trace.push("left-ready");
                    Command::emit(Msg::ParentFollowUp)
                }
                Msg::Right(ChildMsg::Ready) => {
                    self.trace.push("right-ready");
                    Command::none()
                }
                Msg::ParentFollowUp => {
                    self.trace.push("parent-follow-up");
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

    let program: Entity<Program<ParentModel>> = cx.update(|cx| {
        ParentModel {
            left: ReadyChild,
            right: ReadyChild,
            trace: Vec::new(),
        }
        .into_program(cx)
    });

    program.read_with(cx, |program, _cx| {
        assert_eq!(
            program.model().trace,
            vec!["left-ready", "right-ready", "parent-follow-up"]
        );
    });
}

#[gpui::test]
fn composite_update_returns_unmatched_messages_unchanged(cx: &mut TestAppContext) {
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum Msg {
        Child(()),
        ParentOnly(u8),
    }

    impl Msg {
        fn into_child(self) -> Result<(), Self> {
            match self {
                Self::Child(msg) => Ok(msg),
                other => Err(other),
            }
        }
    }

    struct Child;

    impl Model for Child {
        type Msg = ();

        fn update(
            &mut self,
            _msg: Self::Msg,
            _cx: &mut App,
            _scope: &ModelContext<Self::Msg>,
        ) -> Command<Self::Msg> {
            Command::none()
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

    #[derive(Composite)]
    #[composite(message = Msg)]
    struct ParentModel {
        #[child(path = "child", lift = Msg::Child, extract = Msg::into_child)]
        child: Child,
    }

    let mut model = ParentModel { child: Child };
    let result =
        cx.update(|cx| model.__composite_update(Msg::ParentOnly(7), cx, &ModelContext::root()));
    match result {
        Err(Msg::ParentOnly(value)) => assert_eq!(value, 7),
        Ok(_) => panic!("child helper should not match parent-only messages"),
        Err(other) => panic!("unexpected unmatched message: {other:?}"),
    }
}

#[gpui::test]
fn sibling_composite_keyed_commands_keep_distinct_paths(cx: &mut TestAppContext) {
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

    impl Msg {
        fn into_left(self) -> Result<ChildMsg, Self> {
            match self {
                Self::Left(msg) => Ok(msg),
                other => Err(other),
            }
        }

        fn into_right(self) -> Result<ChildMsg, Self> {
            match self {
                Self::Right(msg) => Ok(msg),
                other => Err(other),
            }
        }
    }

    #[derive(Clone, Debug, PartialEq, Eq)]
    struct KeyRecord {
        path: Option<ChildPath>,
        local_id: Option<String>,
    }

    struct LoaderChild {
        value: Option<i32>,
    }

    impl Model for LoaderChild {
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

    #[derive(Composite)]
    #[composite(message = Msg)]
    struct ParentModel {
        #[child(path = "left", lift = Msg::Left, extract = Msg::into_left)]
        left: LoaderChild,
        #[child(path = "right", lift = Msg::Right, extract = Msg::into_right)]
        right: LoaderChild,
    }

    impl Model for ParentModel {
        type Msg = Msg;

        fn update(
            &mut self,
            msg: Self::Msg,
            cx: &mut App,
            scope: &ModelContext<Self::Msg>,
        ) -> Command<Self::Msg> {
            match self.__composite_update(msg, cx, scope) {
                Ok(command) => command,
                Err(other) => panic!("unexpected unmatched message: {other:?}"),
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
fn composite_subscriptions_merge_child_sets_and_keep_scoped_keys(cx: &mut TestAppContext) {
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum ChildMsg {
        FromSubscription(&'static str),
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum Msg {
        Left(ChildMsg),
        Right(ChildMsg),
    }

    impl Msg {
        fn into_left(self) -> Result<ChildMsg, Self> {
            match self {
                Self::Left(msg) => Ok(msg),
                other => Err(other),
            }
        }

        fn into_right(self) -> Result<ChildMsg, Self> {
            match self {
                Self::Right(msg) => Ok(msg),
                other => Err(other),
            }
        }
    }

    struct SubscriptionChild {
        id: &'static str,
    }

    impl Model for SubscriptionChild {
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

    #[derive(Composite)]
    #[composite(message = Msg)]
    struct ParentModel {
        #[child(path = "left", lift = Msg::Left, extract = Msg::into_left)]
        left: SubscriptionChild,
        #[child(path = "right", lift = Msg::Right, extract = Msg::into_right)]
        right: SubscriptionChild,
        delivered: Vec<&'static str>,
    }

    impl Model for ParentModel {
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
            self.__composite_subscriptions(cx, scope)
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
fn composite_view_helpers_route_child_dispatches_through_parent_dispatcher(
    cx: &mut TestAppContext,
) {
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum ChildMsg {
        Rendered,
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum Msg {
        Left(ChildMsg),
    }

    impl Msg {
        fn into_left(self) -> Result<ChildMsg, Self> {
            match self {
                Self::Left(msg) => Ok(msg),
            }
        }
    }

    struct ViewChild {
        render_calls: Arc<AtomicUsize>,
    }

    impl Model for ViewChild {
        type Msg = ChildMsg;

        fn update(
            &mut self,
            _msg: Self::Msg,
            _cx: &mut App,
            _scope: &ModelContext<Self::Msg>,
        ) -> Command<Self::Msg> {
            Command::none()
        }

        fn view(
            &self,
            _window: &mut Window,
            _cx: &mut App,
            _scope: &ModelContext<Self::Msg>,
            dispatcher: &Dispatcher<Self::Msg>,
        ) -> View {
            if self.render_calls.fetch_add(1, Ordering::SeqCst) == 0 {
                dispatcher.dispatch(ChildMsg::Rendered).unwrap();
            }

            div().into_view()
        }
    }

    #[derive(Composite)]
    #[composite(message = Msg)]
    struct ParentModel {
        #[child(path = "left", lift = Msg::Left, extract = Msg::into_left)]
        left: ViewChild,
        delivered: Arc<AtomicUsize>,
    }

    impl Model for ParentModel {
        type Msg = Msg;

        fn update(
            &mut self,
            msg: Self::Msg,
            _cx: &mut App,
            _scope: &ModelContext<Self::Msg>,
        ) -> Command<Self::Msg> {
            match msg {
                Msg::Left(ChildMsg::Rendered) => {
                    self.delivered.fetch_add(1, Ordering::SeqCst);
                }
            }

            Command::none()
        }

        fn view(
            &self,
            window: &mut Window,
            cx: &mut App,
            scope: &ModelContext<Self::Msg>,
            dispatcher: &Dispatcher<Self::Msg>,
        ) -> View {
            self.left_view(window, cx, scope, dispatcher)
        }
    }

    let delivered = Arc::new(AtomicUsize::new(0));
    let render_calls = Arc::new(AtomicUsize::new(0));
    cx.update(|cx| {
        cx.open_window(gpui::WindowOptions::default(), |_, cx| {
            ParentModel {
                left: ViewChild {
                    render_calls: render_calls.clone(),
                },
                delivered: delivered.clone(),
            }
            .into_program(cx)
        })
        .unwrap();
    });

    cx.run_until_parked();

    assert!(render_calls.load(Ordering::SeqCst) >= 1);
    assert_eq!(delivered.load(Ordering::SeqCst), 1);
}

#[gpui::test]
fn deep_nested_composite_scope_rebases_grandchild_keys(cx: &mut TestAppContext) {
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum GrandChildMsg {
        Run,
        Loaded,
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum ChildMsg {
        Grandchild(GrandChildMsg),
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum Msg {
        Child(ChildMsg),
    }

    impl ChildMsg {
        fn into_grandchild(self) -> Result<GrandChildMsg, Self> {
            match self {
                Self::Grandchild(msg) => Ok(msg),
            }
        }
    }

    impl Msg {
        fn into_child(self) -> Result<ChildMsg, Self> {
            match self {
                Self::Child(msg) => Ok(msg),
            }
        }
    }

    struct Grandchild {
        completed: bool,
    }

    impl Model for Grandchild {
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

    #[derive(Composite)]
    #[composite(message = ChildMsg)]
    struct ChildModel {
        #[child(
            path = "grandchild",
            lift = ChildMsg::Grandchild,
            extract = ChildMsg::into_grandchild
        )]
        grandchild: Grandchild,
    }

    impl Model for ChildModel {
        type Msg = ChildMsg;

        fn update(
            &mut self,
            msg: Self::Msg,
            cx: &mut App,
            scope: &ModelContext<Self::Msg>,
        ) -> Command<Self::Msg> {
            match self.__composite_update(msg, cx, scope) {
                Ok(command) => command,
                Err(other) => panic!("unexpected unmatched message: {other:?}"),
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

    #[derive(Composite)]
    #[composite(message = Msg)]
    struct ParentModel {
        #[child(path = "child", lift = Msg::Child, extract = Msg::into_child)]
        child: ChildModel,
    }

    impl Model for ParentModel {
        type Msg = Msg;

        fn update(
            &mut self,
            msg: Self::Msg,
            cx: &mut App,
            scope: &ModelContext<Self::Msg>,
        ) -> Command<Self::Msg> {
            match self.__composite_update(msg, cx, scope) {
                Ok(command) => command,
                Err(other) => panic!("unexpected unmatched message: {other:?}"),
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
        .dispatch(Msg::Child(ChildMsg::Grandchild(GrandChildMsg::Run)))
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

#[gpui::test]
fn telemetry_metadata_uses_stable_program_id_and_monotonic_event_ids(cx: &mut TestAppContext) {
    #[derive(Clone, Debug, PartialEq, Eq)]
    enum Msg {
        Run,
        Loaded,
    }

    #[derive(Clone, Debug, PartialEq, Eq)]
    struct TelemetryRecord {
        program_id: u64,
        event_id: u64,
        queue_depth: usize,
        program_description: Option<String>,
    }

    struct TelemetryModel;

    impl Model for TelemetryModel {
        type Msg = Msg;

        fn update(
            &mut self,
            msg: Self::Msg,
            _cx: &mut App,
            _scope: &ModelContext<Self::Msg>,
        ) -> Command<Self::Msg> {
            match msg {
                Msg::Run => Command::emit(Msg::Loaded).label("load"),
                Msg::Loaded => Command::none(),
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

    let records = Arc::new(Mutex::new(Vec::new()));
    let config = ProgramConfig::default()
        .describe_program(|program_id| format!("program:{program_id}"))
        .telemetry_observer({
            let records = records.clone();
            move |envelope: TelemetryEnvelope<'_, Msg>| {
                records.lock().unwrap().push(TelemetryRecord {
                    program_id: envelope.metadata.program_id.get(),
                    event_id: envelope.metadata.event_id,
                    queue_depth: envelope.metadata.queue_depth,
                    program_description: envelope
                        .metadata
                        .program_description
                        .map(|value| value.to_string()),
                });
            }
        });

    let program_a: Entity<Program<TelemetryModel>> =
        cx.update(|cx| TelemetryModel.into_program_with(config.clone(), cx));
    let program_b: Entity<Program<TelemetryModel>> =
        cx.update(|cx| TelemetryModel.into_program_with(config, cx));
    let dispatcher_a = program_a.read_with(cx, |program, _cx| program.dispatcher());
    let dispatcher_b = program_b.read_with(cx, |program, _cx| program.dispatcher());

    dispatcher_a.dispatch(Msg::Run).unwrap();
    cx.run_until_parked();
    dispatcher_b.dispatch(Msg::Run).unwrap();
    cx.run_until_parked();

    let records = records.lock().unwrap();
    assert!(!records.is_empty());
    let mut descriptions_by_program = std::collections::BTreeMap::new();
    for record in records.iter() {
        descriptions_by_program
            .entry(record.program_id)
            .or_insert_with(Vec::new)
            .push(record.program_description.clone());
    }
    assert_eq!(descriptions_by_program.len(), 2);
    assert!(
        records
            .windows(2)
            .all(|pair| pair[0].event_id < pair[1].event_id)
    );
    for (program_id, descriptions) in descriptions_by_program {
        assert!(
            descriptions
                .iter()
                .all(|description| description.as_deref() == Some(&format!("program:{program_id}")))
        );
    }
    assert!(records.iter().any(|record| record.queue_depth == 0));
}

#[gpui::test]
fn drop_oldest_via_mounted_program_records_displaced_message_description(cx: &mut TestAppContext) {
    #[derive(Clone, Debug, PartialEq, Eq)]
    enum Msg {
        Set(i32),
    }

    struct QueueModel {
        received: Vec<i32>,
    }

    impl Model for QueueModel {
        type Msg = Msg;

        fn update(
            &mut self,
            msg: Self::Msg,
            _cx: &mut App,
            _scope: &ModelContext<Self::Msg>,
        ) -> Command<Self::Msg> {
            match msg {
                Msg::Set(val) => self.received.push(val),
            }
            Command::none()
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

    let telemetry_events = Arc::new(Mutex::new(Vec::new()));

    let config = ProgramConfig::default()
        .queue_policy(QueuePolicy::DropOldest { capacity: 2 })
        .describe_message(|msg: &Msg| match msg {
            Msg::Set(v) => format!("set:{v}"),
        })
        .telemetry_observer({
            let telemetry_events = telemetry_events.clone();
            move |envelope| {
                if let TelemetryEvent::QueueOverflow {
                    action,
                    message_description,
                    ..
                } = &envelope.event
                {
                    telemetry_events
                        .lock()
                        .unwrap()
                        .push((*action, message_description.clone()));
                }
            }
        });

    let program: Entity<Program<QueueModel>> = cx.update(|cx| {
        QueueModel {
            received: Vec::new(),
        }
        .into_program_with(config, cx)
    });
    let dispatcher = program.read_with(cx, |program, _cx| program.dispatcher());

    dispatcher.dispatch(Msg::Set(1)).unwrap();
    dispatcher.dispatch(Msg::Set(2)).unwrap();
    dispatcher.dispatch(Msg::Set(3)).unwrap();

    cx.run_until_parked();

    program.read_with(cx, |program, _cx| {
        assert_eq!(program.model().received.as_slice(), &[2, 3]);
    });

    let overflows = telemetry_events.lock().unwrap();
    assert_eq!(overflows.len(), 1);
    assert_eq!(overflows[0].0, QueueOverflowAction::DroppedOldest);
    assert_eq!(overflows[0].1.as_deref(), Some("set:1"));
}

#[gpui::test]
fn drop_newest_via_mounted_program_records_dropped_message_description(cx: &mut TestAppContext) {
    #[derive(Clone, Debug, PartialEq, Eq)]
    enum Msg {
        Set(i32),
    }

    struct QueueModel {
        received: Vec<i32>,
    }

    impl Model for QueueModel {
        type Msg = Msg;

        fn update(
            &mut self,
            msg: Self::Msg,
            _cx: &mut App,
            _scope: &ModelContext<Self::Msg>,
        ) -> Command<Self::Msg> {
            match msg {
                Msg::Set(val) => self.received.push(val),
            }
            Command::none()
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

    let telemetry_events = Arc::new(Mutex::new(Vec::new()));

    let config = ProgramConfig::default()
        .queue_policy(QueuePolicy::DropNewest { capacity: 2 })
        .describe_message(|msg: &Msg| match msg {
            Msg::Set(v) => format!("set:{v}"),
        })
        .telemetry_observer({
            let telemetry_events = telemetry_events.clone();
            move |envelope| {
                if let TelemetryEvent::QueueOverflow {
                    action,
                    message_description,
                    ..
                } = &envelope.event
                {
                    telemetry_events
                        .lock()
                        .unwrap()
                        .push((*action, message_description.clone()));
                }
            }
        });

    let program: Entity<Program<QueueModel>> = cx.update(|cx| {
        QueueModel {
            received: Vec::new(),
        }
        .into_program_with(config, cx)
    });
    let dispatcher = program.read_with(cx, |program, _cx| program.dispatcher());

    dispatcher.dispatch(Msg::Set(1)).unwrap();
    dispatcher.dispatch(Msg::Set(2)).unwrap();
    dispatcher.dispatch(Msg::Set(3)).unwrap();

    cx.run_until_parked();

    program.read_with(cx, |program, _cx| {
        assert_eq!(program.model().received.as_slice(), &[1, 2]);
    });

    let overflows = telemetry_events.lock().unwrap();
    assert_eq!(overflows.len(), 1);
    assert_eq!(overflows[0].0, QueueOverflowAction::DroppedNewest);
    assert_eq!(overflows[0].1.as_deref(), Some("set:3"));
}

#[gpui::test]
fn cancel_key_command_emits_command_scheduled_event_with_label_and_key(cx: &mut TestAppContext) {
    #[derive(Clone, Debug, PartialEq, Eq)]
    enum Msg {
        CancelTask,
    }

    struct CancelModel;

    impl Model for CancelModel {
        type Msg = Msg;

        fn update(
            &mut self,
            msg: Self::Msg,
            _cx: &mut App,
            _scope: &ModelContext<Self::Msg>,
        ) -> Command<Self::Msg> {
            match msg {
                Msg::CancelTask => Command::cancel_key("task-key").label("stop-task"),
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

    let scheduled_events = Arc::new(Mutex::new(Vec::new()));

    let config = ProgramConfig::default()
        .describe_key(|key| format!("{key:?}"))
        .telemetry_observer({
            let scheduled_events = scheduled_events.clone();
            move |envelope| {
                if let TelemetryEvent::CommandScheduled {
                    kind,
                    label,
                    key,
                    key_description,
                } = &envelope.event
                {
                    scheduled_events.lock().unwrap().push((
                        *kind,
                        label.map(ToString::to_string),
                        key.map(|k| k.id().to_string()),
                        key_description.as_deref().map(ToString::to_string),
                    ));
                }
            }
        });

    let program: Entity<Program<CancelModel>> =
        cx.update(|cx| CancelModel.into_program_with(config, cx));
    let dispatcher = program.read_with(cx, |program, _cx| program.dispatcher());

    dispatcher.dispatch(Msg::CancelTask).unwrap();
    cx.run_until_parked();

    let scheduled = scheduled_events.lock().unwrap();
    assert!(scheduled.iter().any(|(kind, label, key, key_desc)| {
        *kind == CommandKind::Cancel
            && label.as_deref() == Some("stop-task")
            && key.as_deref() == Some("task-key")
            && key_desc.as_deref() == Some("Key(\"task-key\")")
    }));
}

#[gpui::test]
fn keyed_effects_are_canceled_when_program_is_dropped(cx: &mut TestAppContext) {
    #[derive(Clone, Debug, PartialEq, Eq)]
    enum Msg {
        Run,
        Loaded(i32),
    }

    struct KeyedDropModel {
        receiver: Option<oneshot::Receiver<i32>>,
        loaded: Vec<i32>,
    }

    impl Model for KeyedDropModel {
        type Msg = Msg;

        fn update(
            &mut self,
            msg: Self::Msg,
            _cx: &mut App,
            _scope: &ModelContext<Self::Msg>,
        ) -> Command<Self::Msg> {
            match msg {
                Msg::Run => {
                    let receiver = self
                        .receiver
                        .take()
                        .expect("run should only be dispatched once");
                    Command::background_keyed("load_key", move |_| async move {
                        receiver.await.ok().map(Msg::Loaded)
                    })
                }
                Msg::Loaded(val) => {
                    self.loaded.push(val);
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

    let (sender, receiver) = oneshot::channel();
    type KeyedProgram = Entity<Program<KeyedDropModel>>;

    let program_slot: Arc<Mutex<Option<KeyedProgram>>> = Arc::new(Mutex::new(None));
    let window = cx.update(|cx| {
        let program_slot = program_slot.clone();
        cx.open_window(gpui::WindowOptions::default(), move |_, cx| {
            let program = Program::mount(
                KeyedDropModel {
                    receiver: Some(receiver),
                    loaded: Vec::new(),
                },
                cx,
            );
            *program_slot.lock().unwrap() = Some(program.clone());
            program
        })
        .unwrap()
    });

    let program = program_slot.lock().unwrap().take().unwrap();
    let dispatcher = program.read_with(cx, |program, _cx| program.dispatcher());
    drop(program);

    cx.executor().allow_parking();
    dispatcher.dispatch(Msg::Run).unwrap();
    cx.run_until_parked();

    cx.update(|cx| {
        window
            .update(cx, |_, window, _| window.remove_window())
            .unwrap();
    });
    cx.run_until_parked();
    cx.executor().forbid_parking();

    assert_eq!(sender.send(99), Err(99));
}

#[gpui::test]
fn subscriptions_are_canceled_when_program_is_dropped(cx: &mut TestAppContext) {
    #[derive(Clone, Debug, PartialEq, Eq)]
    enum Msg {}

    struct SubDropModel {
        receiver: Arc<Mutex<Option<oneshot::Receiver<()>>>>,
    }

    impl Model for SubDropModel {
        type Msg = Msg;

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
            let receiver = self.receiver.lock().unwrap().take();
            Subscriptions::one(Subscription::new("active_sub", move |sub_cx| {
                if let Some(receiver) = receiver {
                    let task = sub_cx.app().background_executor().spawn(async move {
                        let _ = receiver.await;
                    });
                    SubHandle::Task(task)
                } else {
                    SubHandle::None
                }
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

    let (sender, receiver) = oneshot::channel();
    type SubProgram = Entity<Program<SubDropModel>>;

    let program_slot: Arc<Mutex<Option<SubProgram>>> = Arc::new(Mutex::new(None));
    let window = cx.update(|cx| {
        let program_slot = program_slot.clone();
        cx.open_window(gpui::WindowOptions::default(), move |_, cx| {
            let program = Program::mount(
                SubDropModel {
                    receiver: Arc::new(Mutex::new(Some(receiver))),
                },
                cx,
            );
            *program_slot.lock().unwrap() = Some(program.clone());
            program
        })
        .unwrap()
    });

    let program = program_slot.lock().unwrap().take().unwrap();
    drop(program);

    cx.executor().allow_parking();
    cx.run_until_parked();

    cx.update(|cx| {
        window
            .update(cx, |_, window, _| window.remove_window())
            .unwrap();
    });
    cx.run_until_parked();
    cx.executor().forbid_parking();

    assert_eq!(sender.send(()), Err(()));
}

#[gpui::test]
fn drop_oldest_via_internal_emits_preserves_all_messages(cx: &mut TestAppContext) {
    #[derive(Clone, Debug, PartialEq, Eq)]
    enum Msg {
        Trigger,
        Step(i32),
    }

    struct InternalModel {
        processed: Vec<i32>,
    }

    impl Model for InternalModel {
        type Msg = Msg;

        fn update(
            &mut self,
            msg: Self::Msg,
            _cx: &mut App,
            _scope: &ModelContext<Self::Msg>,
        ) -> Command<Self::Msg> {
            match msg {
                Msg::Trigger => Command::batch([
                    Command::emit(Msg::Step(1)),
                    Command::emit(Msg::Step(2)),
                    Command::emit(Msg::Step(3)),
                ]),
                Msg::Step(val) => {
                    self.processed.push(val);
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

    let overflow_events = Arc::new(Mutex::new(Vec::new()));

    let config = ProgramConfig::default()
        .queue_policy(QueuePolicy::DropOldest { capacity: 2 })
        .describe_message(|msg: &Msg| match msg {
            Msg::Trigger => String::from("trigger"),
            Msg::Step(v) => format!("step:{v}"),
        })
        .telemetry_observer({
            let overflow_events = overflow_events.clone();
            move |envelope| {
                if let TelemetryEvent::QueueOverflow {
                    action,
                    message_description,
                    ..
                } = &envelope.event
                {
                    overflow_events
                        .lock()
                        .unwrap()
                        .push((*action, message_description.clone()));
                }
            }
        });

    let program: Entity<Program<InternalModel>> = cx.update(|cx| {
        InternalModel {
            processed: Vec::new(),
        }
        .into_program_with(config, cx)
    });
    let dispatcher = program.read_with(cx, |program, _cx| program.dispatcher());

    dispatcher.dispatch(Msg::Trigger).unwrap();
    cx.run_until_parked();

    program.read_with(cx, |program, _cx| {
        // Internal emits have guaranteed delivery and are never dropped
        assert_eq!(program.model().processed.as_slice(), &[1, 2, 3]);
    });

    let overflows = overflow_events.lock().unwrap();
    assert!(
        overflows.is_empty(),
        "internal commands must never produce overflow events"
    );
}

#[gpui::test]
fn drop_newest_via_internal_emits_preserves_all_messages(cx: &mut TestAppContext) {
    #[derive(Clone, Debug, PartialEq, Eq)]
    enum Msg {
        Trigger,
        Step(i32),
    }

    struct InternalModel {
        processed: Vec<i32>,
    }

    impl Model for InternalModel {
        type Msg = Msg;

        fn update(
            &mut self,
            msg: Self::Msg,
            _cx: &mut App,
            _scope: &ModelContext<Self::Msg>,
        ) -> Command<Self::Msg> {
            match msg {
                Msg::Trigger => Command::batch([
                    Command::emit(Msg::Step(1)),
                    Command::emit(Msg::Step(2)),
                    Command::emit(Msg::Step(3)),
                ]),
                Msg::Step(val) => {
                    self.processed.push(val);
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

    let overflow_events = Arc::new(Mutex::new(Vec::new()));

    let config = ProgramConfig::default()
        .queue_policy(QueuePolicy::DropNewest { capacity: 2 })
        .describe_message(|msg: &Msg| match msg {
            Msg::Trigger => String::from("trigger"),
            Msg::Step(v) => format!("step:{v}"),
        })
        .telemetry_observer({
            let overflow_events = overflow_events.clone();
            move |envelope| {
                if let TelemetryEvent::QueueOverflow {
                    action,
                    message_description,
                    ..
                } = &envelope.event
                {
                    overflow_events
                        .lock()
                        .unwrap()
                        .push((*action, message_description.clone()));
                }
            }
        });

    let program: Entity<Program<InternalModel>> = cx.update(|cx| {
        InternalModel {
            processed: Vec::new(),
        }
        .into_program_with(config, cx)
    });
    let dispatcher = program.read_with(cx, |program, _cx| program.dispatcher());

    dispatcher.dispatch(Msg::Trigger).unwrap();
    cx.run_until_parked();

    program.read_with(cx, |program, _cx| {
        // Internal emits have guaranteed delivery and are never dropped
        assert_eq!(program.model().processed.as_slice(), &[1, 2, 3]);
    });

    let overflows = overflow_events.lock().unwrap();
    assert!(
        overflows.is_empty(),
        "internal commands must never be dropped under DropNewest"
    );
}

#[gpui::test]
fn all_internal_command_kinds_delivered_when_external_queue_is_zero_capacity_reject_new(
    cx: &mut TestAppContext,
) {
    #[derive(Clone, Debug, PartialEq, Eq)]
    enum Msg {
        Step(&'static str),
    }

    struct AllEffectsModel {
        processed: Vec<&'static str>,
    }

    impl Model for AllEffectsModel {
        type Msg = Msg;

        fn init(&mut self, _cx: &mut App, _scope: &ModelContext<Self::Msg>) -> Command<Self::Msg> {
            Command::batch([
                Command::emit(Msg::Step("emit")),
                Command::foreground(|_: &mut gpui::AsyncApp| async {
                    Some(Msg::Step("foreground"))
                }),
                Command::background(|_executor| async { Some(Msg::Step("background")) }),
                Command::foreground_keyed("k1", |_: &mut gpui::AsyncApp| async {
                    Some(Msg::Step("fg_keyed"))
                }),
                Command::background_keyed("k2", |_executor| async { Some(Msg::Step("bg_keyed")) }),
            ])
        }

        fn update(
            &mut self,
            msg: Self::Msg,
            _cx: &mut App,
            _scope: &ModelContext<Self::Msg>,
        ) -> Command<Self::Msg> {
            match msg {
                Msg::Step(val) => {
                    self.processed.push(val);
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

    let config = ProgramConfig::default().queue_policy(QueuePolicy::RejectNew { capacity: 0 });

    let program: Entity<Program<AllEffectsModel>> = cx.update(|cx| {
        AllEffectsModel {
            processed: Vec::new(),
        }
        .into_program_with(config, cx)
    });
    let dispatcher = program.read_with(cx, |program, _cx| program.dispatcher());

    // External dispatch must fail because capacity is 0:
    let err = dispatcher.dispatch(Msg::Step("external")).unwrap_err();
    assert!(matches!(err, gpui_tea::Error::QueueFull { .. }));

    // Run all internal commands to completion
    cx.run_until_parked();

    program.read_with(cx, |program, _cx| {
        assert_eq!(program.model().processed.len(), 5);
        assert!(program.model().processed.contains(&"emit"));
        assert!(program.model().processed.contains(&"foreground"));
        assert!(program.model().processed.contains(&"background"));
        assert!(program.model().processed.contains(&"fg_keyed"));
        assert!(program.model().processed.contains(&"bg_keyed"));
    });
}

#[gpui::test]
fn drop_oldest_displaces_only_external_messages_protecting_internal_in_pending(
    cx: &mut TestAppContext,
) {
    #[derive(Clone, Debug, PartialEq, Eq)]
    enum Msg {
        External(&'static str),
        Internal(&'static str),
    }

    struct FifoModel {
        processed: Vec<&'static str>,
    }

    impl Model for FifoModel {
        type Msg = Msg;

        fn init(&mut self, _cx: &mut App, _scope: &ModelContext<Self::Msg>) -> Command<Self::Msg> {
            // Internal message enqueued during init sits in pending
            Command::emit(Msg::Internal("i1"))
        }

        fn update(
            &mut self,
            msg: Self::Msg,
            _cx: &mut App,
            _scope: &ModelContext<Self::Msg>,
        ) -> Command<Self::Msg> {
            match msg {
                Msg::External(name) => {
                    self.processed.push(name);
                    Command::none()
                }
                Msg::Internal(name) => {
                    self.processed.push(name);
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

    let overflow_events = Arc::new(Mutex::new(Vec::new()));
    let config = ProgramConfig::default()
        .queue_policy(QueuePolicy::DropOldest { capacity: 1 })
        .describe_message(|msg: &Msg| match msg {
            Msg::External(n) => format!("ext:{n}"),
            Msg::Internal(n) => format!("int:{n}"),
        })
        .telemetry_observer({
            let overflow_events = overflow_events.clone();
            move |envelope| {
                if let TelemetryEvent::QueueOverflow {
                    action,
                    message_description,
                    ..
                } = &envelope.event
                {
                    overflow_events
                        .lock()
                        .unwrap()
                        .push((*action, message_description.clone()));
                }
            }
        });

    let program: Entity<Program<FifoModel>> = cx.update(|cx| {
        FifoModel {
            processed: Vec::new(),
        }
        .into_program_with(config, cx)
    });
    let dispatcher = program.read_with(cx, |program, _cx| program.dispatcher());

    // Dispatch E1
    dispatcher.dispatch(Msg::External("e1")).unwrap();
    // Dispatch E2 while E1 is active in capacity-1 external pool -> E2 displaces E1!
    dispatcher.dispatch(Msg::External("e2")).unwrap();

    cx.run_until_parked();

    // E1 was displaced by E2.
    // Internal message i1 was protected in pending and processed!
    // E2 was processed!
    let overflows = overflow_events.lock().unwrap();
    assert_eq!(overflows.len(), 1);
    assert_eq!(overflows[0].0, QueueOverflowAction::DroppedOldest);
    assert_eq!(overflows[0].1.as_deref(), Some("ext:e1"));

    program.read_with(cx, |program, _cx| {
        assert_eq!(program.model().processed.as_slice(), &["i1", "e2"]);
    });
}

#[gpui::test]
fn all_internal_command_kinds_delivered_when_external_queue_is_zero_capacity_drop_newest(
    cx: &mut TestAppContext,
) {
    #[derive(Clone, Debug, PartialEq, Eq)]
    enum Msg {
        Step(&'static str),
    }

    struct AllEffectsModel {
        processed: Vec<&'static str>,
    }

    impl Model for AllEffectsModel {
        type Msg = Msg;

        fn init(&mut self, _cx: &mut App, _scope: &ModelContext<Self::Msg>) -> Command<Self::Msg> {
            Command::batch([
                Command::emit(Msg::Step("emit")),
                Command::foreground(|_: &mut gpui::AsyncApp| async {
                    Some(Msg::Step("foreground"))
                }),
                Command::background(|_executor| async { Some(Msg::Step("background")) }),
                Command::foreground_keyed("k1", |_: &mut gpui::AsyncApp| async {
                    Some(Msg::Step("fg_keyed"))
                }),
                Command::background_keyed("k2", |_executor| async { Some(Msg::Step("bg_keyed")) }),
            ])
        }

        fn update(
            &mut self,
            msg: Self::Msg,
            _cx: &mut App,
            _scope: &ModelContext<Self::Msg>,
        ) -> Command<Self::Msg> {
            match msg {
                Msg::Step(val) => {
                    self.processed.push(val);
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

    let config = ProgramConfig::default().queue_policy(QueuePolicy::DropNewest { capacity: 0 });

    let program: Entity<Program<AllEffectsModel>> = cx.update(|cx| {
        AllEffectsModel {
            processed: Vec::new(),
        }
        .into_program_with(config, cx)
    });
    let dispatcher = program.read_with(cx, |program, _cx| program.dispatcher());

    // External dispatch is dropped under capacity 0:
    dispatcher.dispatch(Msg::Step("external")).unwrap();

    // Run all internal commands to completion
    cx.run_until_parked();

    program.read_with(cx, |program, _cx| {
        assert_eq!(program.model().processed.len(), 5);
        assert!(program.model().processed.contains(&"emit"));
        assert!(program.model().processed.contains(&"foreground"));
        assert!(program.model().processed.contains(&"background"));
        assert!(program.model().processed.contains(&"fg_keyed"));
        assert!(program.model().processed.contains(&"bg_keyed"));
        assert!(!program.model().processed.contains(&"external"));
    });
}

#[gpui::test]
fn all_internal_command_kinds_delivered_when_external_queue_is_zero_capacity_drop_oldest(
    cx: &mut TestAppContext,
) {
    #[derive(Clone, Debug, PartialEq, Eq)]
    enum Msg {
        Step(&'static str),
    }

    struct AllEffectsModel {
        processed: Vec<&'static str>,
    }

    impl Model for AllEffectsModel {
        type Msg = Msg;

        fn init(&mut self, _cx: &mut App, _scope: &ModelContext<Self::Msg>) -> Command<Self::Msg> {
            Command::batch([
                Command::emit(Msg::Step("emit")),
                Command::foreground(|_: &mut gpui::AsyncApp| async {
                    Some(Msg::Step("foreground"))
                }),
                Command::background(|_executor| async { Some(Msg::Step("background")) }),
                Command::foreground_keyed("k1", |_: &mut gpui::AsyncApp| async {
                    Some(Msg::Step("fg_keyed"))
                }),
                Command::background_keyed("k2", |_executor| async { Some(Msg::Step("bg_keyed")) }),
            ])
        }

        fn update(
            &mut self,
            msg: Self::Msg,
            _cx: &mut App,
            _scope: &ModelContext<Self::Msg>,
        ) -> Command<Self::Msg> {
            match msg {
                Msg::Step(val) => {
                    self.processed.push(val);
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

    let config = ProgramConfig::default().queue_policy(QueuePolicy::DropOldest { capacity: 0 });

    let program: Entity<Program<AllEffectsModel>> = cx.update(|cx| {
        AllEffectsModel {
            processed: Vec::new(),
        }
        .into_program_with(config, cx)
    });
    let dispatcher = program.read_with(cx, |program, _cx| program.dispatcher());

    // Dispatch external messages; under capacity 0, newest displaces previous:
    dispatcher.dispatch(Msg::Step("ext1")).unwrap();
    dispatcher.dispatch(Msg::Step("ext2")).unwrap();

    cx.run_until_parked();

    program.read_with(cx, |program, _cx| {
        assert_eq!(program.model().processed.len(), 6);
        assert!(program.model().processed.contains(&"emit"));
        assert!(program.model().processed.contains(&"foreground"));
        assert!(program.model().processed.contains(&"background"));
        assert!(program.model().processed.contains(&"fg_keyed"));
        assert!(program.model().processed.contains(&"bg_keyed"));
        assert!(program.model().processed.contains(&"ext2"));
        assert!(!program.model().processed.contains(&"ext1"));
    });
}

#[gpui::test]
fn all_internal_command_kinds_delivered_when_external_queue_is_saturated_reject_new(
    cx: &mut TestAppContext,
) {
    #[derive(Clone, Debug, PartialEq, Eq)]
    enum Msg {
        Step(&'static str),
        TriggerAll,
    }

    struct SaturatedModel {
        processed: Vec<&'static str>,
    }

    impl Model for SaturatedModel {
        type Msg = Msg;

        fn update(
            &mut self,
            msg: Self::Msg,
            _cx: &mut App,
            _scope: &ModelContext<Self::Msg>,
        ) -> Command<Self::Msg> {
            match msg {
                Msg::Step(val) => {
                    self.processed.push(val);
                    Command::none()
                }
                Msg::TriggerAll => Command::batch([
                    Command::emit(Msg::Step("emit")),
                    Command::foreground(|_: &mut gpui::AsyncApp| async {
                        Some(Msg::Step("foreground"))
                    }),
                    Command::background(|_executor| async { Some(Msg::Step("background")) }),
                    Command::foreground_keyed("k1", |_: &mut gpui::AsyncApp| async {
                        Some(Msg::Step("fg_keyed"))
                    }),
                    Command::background_keyed("k2", |_executor| async {
                        Some(Msg::Step("bg_keyed"))
                    }),
                ]),
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

    let config = ProgramConfig::default().queue_policy(QueuePolicy::RejectNew { capacity: 2 });

    let program: Entity<Program<SaturatedModel>> = cx.update(|cx| {
        SaturatedModel {
            processed: Vec::new(),
        }
        .into_program_with(config, cx)
    });
    let dispatcher = program.read_with(cx, |program, _cx| program.dispatcher());

    // Saturate external queue: 2 capacity
    dispatcher.dispatch(Msg::Step("ext1")).unwrap();
    dispatcher.dispatch(Msg::TriggerAll).unwrap();
    let err = dispatcher.dispatch(Msg::Step("ext_rejected")).unwrap_err();
    assert!(matches!(err, gpui_tea::Error::QueueFull { .. }));

    cx.run_until_parked();

    program.read_with(cx, |program, _cx| {
        assert_eq!(program.model().processed.len(), 6);
        assert!(program.model().processed.contains(&"ext1"));
        assert!(program.model().processed.contains(&"emit"));
        assert!(program.model().processed.contains(&"foreground"));
        assert!(program.model().processed.contains(&"background"));
        assert!(program.model().processed.contains(&"fg_keyed"));
        assert!(program.model().processed.contains(&"bg_keyed"));
        assert!(!program.model().processed.contains(&"ext_rejected"));
    });
}

#[gpui::test]
fn all_internal_command_kinds_delivered_when_external_queue_is_saturated_drop_newest(
    cx: &mut TestAppContext,
) {
    #[derive(Clone, Debug, PartialEq, Eq)]
    enum Msg {
        Step(&'static str),
        TriggerAll,
    }

    struct SaturatedModel {
        processed: Vec<&'static str>,
    }

    impl Model for SaturatedModel {
        type Msg = Msg;

        fn update(
            &mut self,
            msg: Self::Msg,
            _cx: &mut App,
            _scope: &ModelContext<Self::Msg>,
        ) -> Command<Self::Msg> {
            match msg {
                Msg::Step(val) => {
                    self.processed.push(val);
                    Command::none()
                }
                Msg::TriggerAll => Command::batch([
                    Command::emit(Msg::Step("emit")),
                    Command::foreground(|_: &mut gpui::AsyncApp| async {
                        Some(Msg::Step("foreground"))
                    }),
                    Command::background(|_executor| async { Some(Msg::Step("background")) }),
                    Command::foreground_keyed("k1", |_: &mut gpui::AsyncApp| async {
                        Some(Msg::Step("fg_keyed"))
                    }),
                    Command::background_keyed("k2", |_executor| async {
                        Some(Msg::Step("bg_keyed"))
                    }),
                ]),
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

    let config = ProgramConfig::default().queue_policy(QueuePolicy::DropNewest { capacity: 2 });

    let program: Entity<Program<SaturatedModel>> = cx.update(|cx| {
        SaturatedModel {
            processed: Vec::new(),
        }
        .into_program_with(config, cx)
    });
    let dispatcher = program.read_with(cx, |program, _cx| program.dispatcher());

    // Saturate external queue: 2 capacity
    dispatcher.dispatch(Msg::Step("ext1")).unwrap();
    dispatcher.dispatch(Msg::TriggerAll).unwrap();
    // 3rd external dispatch is dropped under DropNewest
    dispatcher.dispatch(Msg::Step("ext_dropped")).unwrap();

    cx.run_until_parked();

    program.read_with(cx, |program, _cx| {
        assert_eq!(program.model().processed.len(), 6);
        assert!(program.model().processed.contains(&"ext1"));
        assert!(program.model().processed.contains(&"emit"));
        assert!(program.model().processed.contains(&"foreground"));
        assert!(program.model().processed.contains(&"background"));
        assert!(program.model().processed.contains(&"fg_keyed"));
        assert!(program.model().processed.contains(&"bg_keyed"));
        assert!(!program.model().processed.contains(&"ext_dropped"));
    });
}

#[gpui::test]
fn all_internal_command_kinds_delivered_when_external_queue_is_saturated_drop_oldest(
    cx: &mut TestAppContext,
) {
    #[derive(Clone, Debug, PartialEq, Eq)]
    enum Msg {
        Step(&'static str),
        TriggerAll,
    }

    struct SaturatedModel {
        processed: Vec<&'static str>,
    }

    impl Model for SaturatedModel {
        type Msg = Msg;

        fn update(
            &mut self,
            msg: Self::Msg,
            _cx: &mut App,
            _scope: &ModelContext<Self::Msg>,
        ) -> Command<Self::Msg> {
            match msg {
                Msg::Step(val) => {
                    self.processed.push(val);
                    Command::none()
                }
                Msg::TriggerAll => Command::batch([
                    Command::emit(Msg::Step("emit")),
                    Command::foreground(|_: &mut gpui::AsyncApp| async {
                        Some(Msg::Step("foreground"))
                    }),
                    Command::background(|_executor| async { Some(Msg::Step("background")) }),
                    Command::foreground_keyed("k1", |_: &mut gpui::AsyncApp| async {
                        Some(Msg::Step("fg_keyed"))
                    }),
                    Command::background_keyed("k2", |_executor| async {
                        Some(Msg::Step("bg_keyed"))
                    }),
                ]),
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

    let config = ProgramConfig::default().queue_policy(QueuePolicy::DropOldest { capacity: 2 });

    let program: Entity<Program<SaturatedModel>> = cx.update(|cx| {
        SaturatedModel {
            processed: Vec::new(),
        }
        .into_program_with(config, cx)
    });
    let dispatcher = program.read_with(cx, |program, _cx| program.dispatcher());

    // Saturate external queue: 2 capacity
    dispatcher.dispatch(Msg::Step("ext1")).unwrap();
    dispatcher.dispatch(Msg::TriggerAll).unwrap();
    // 3rd external dispatch displaces ext1 under DropOldest
    dispatcher.dispatch(Msg::Step("ext3")).unwrap();

    cx.run_until_parked();

    program.read_with(cx, |program, _cx| {
        // ext1 was displaced. ext3, and all 5 internal commands were delivered!
        assert_eq!(program.model().processed.len(), 6);
        assert!(!program.model().processed.contains(&"ext1"));
        assert!(program.model().processed.contains(&"ext3"));
        assert!(program.model().processed.contains(&"emit"));
        assert!(program.model().processed.contains(&"foreground"));
        assert!(program.model().processed.contains(&"background"));
        assert!(program.model().processed.contains(&"fg_keyed"));
        assert!(program.model().processed.contains(&"bg_keyed"));
    });
}

#[gpui::test]
fn interleaved_internal_and_external_preserves_fifo_order_under_drop_oldest(
    cx: &mut TestAppContext,
) {
    #[derive(Clone, Debug, PartialEq, Eq)]
    enum Msg {
        Step(&'static str),
    }

    struct InterleaveModel {
        processed: Vec<&'static str>,
    }

    impl Model for InterleaveModel {
        type Msg = Msg;

        fn init(&mut self, _cx: &mut App, _scope: &ModelContext<Self::Msg>) -> Command<Self::Msg> {
            Command::batch([
                Command::emit(Msg::Step("i1")),
                Command::emit(Msg::Step("i2")),
            ])
        }

        fn update(
            &mut self,
            msg: Self::Msg,
            _cx: &mut App,
            _scope: &ModelContext<Self::Msg>,
        ) -> Command<Self::Msg> {
            match msg {
                Msg::Step(name) => {
                    self.processed.push(name);
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

    let config = ProgramConfig::default().queue_policy(QueuePolicy::DropOldest { capacity: 1 });

    let program: Entity<Program<InterleaveModel>> = cx.update(|cx| {
        InterleaveModel {
            processed: Vec::new(),
        }
        .into_program_with(config, cx)
    });
    let dispatcher = program.read_with(cx, |program, _cx| program.dispatcher());

    // External dispatch e1
    dispatcher.dispatch(Msg::Step("e1")).unwrap();
    // External dispatch e2 under capacity 1: displaces e1 in the external pool
    dispatcher.dispatch(Msg::Step("e2")).unwrap();

    cx.run_until_parked();

    program.read_with(cx, |program, _cx| {
        // Strict FIFO order: i1, i2 (from init), then e2 (e1 was displaced by e2)
        assert_eq!(program.model().processed.as_slice(), &["i1", "i2", "e2"]);
    });
}

#[cfg(feature = "metrics")]
struct TestMetricHandle {
    key: metrics::Key,
    seen: Arc<Mutex<Vec<metrics::Key>>>,
}

#[cfg(feature = "metrics")]
impl metrics::CounterFn for TestMetricHandle {
    fn increment(&self, _value: u64) {
        self.seen.lock().unwrap().push(self.key.clone());
    }
    fn absolute(&self, _value: u64) {
        self.seen.lock().unwrap().push(self.key.clone());
    }
}

#[cfg(feature = "metrics")]
impl metrics::GaugeFn for TestMetricHandle {
    fn increment(&self, _value: f64) {
        self.seen.lock().unwrap().push(self.key.clone());
    }
    fn decrement(&self, _value: f64) {
        self.seen.lock().unwrap().push(self.key.clone());
    }
    fn set(&self, _value: f64) {
        self.seen.lock().unwrap().push(self.key.clone());
    }
}

#[cfg(feature = "metrics")]
impl metrics::HistogramFn for TestMetricHandle {
    fn record(&self, _value: f64) {
        self.seen.lock().unwrap().push(self.key.clone());
    }
}

#[cfg(feature = "metrics")]
struct TestMetricRecorder {
    seen: Arc<Mutex<Vec<metrics::Key>>>,
}

#[cfg(feature = "metrics")]
impl metrics::Recorder for TestMetricRecorder {
    fn describe_counter(
        &self,
        _: metrics::KeyName,
        _: Option<metrics::Unit>,
        _: metrics::SharedString,
    ) {
    }
    fn describe_gauge(
        &self,
        _: metrics::KeyName,
        _: Option<metrics::Unit>,
        _: metrics::SharedString,
    ) {
    }
    fn describe_histogram(
        &self,
        _: metrics::KeyName,
        _: Option<metrics::Unit>,
        _: metrics::SharedString,
    ) {
    }
    fn register_counter(&self, key: &metrics::Key, _: &metrics::Metadata<'_>) -> metrics::Counter {
        metrics::Counter::from_arc(Arc::new(TestMetricHandle {
            key: key.clone(),
            seen: self.seen.clone(),
        }))
    }
    fn register_gauge(&self, key: &metrics::Key, _: &metrics::Metadata<'_>) -> metrics::Gauge {
        metrics::Gauge::from_arc(Arc::new(TestMetricHandle {
            key: key.clone(),
            seen: self.seen.clone(),
        }))
    }
    fn register_histogram(
        &self,
        key: &metrics::Key,
        _: &metrics::Metadata<'_>,
    ) -> metrics::Histogram {
        metrics::Histogram::from_arc(Arc::new(TestMetricHandle {
            key: key.clone(),
            seen: self.seen.clone(),
        }))
    }
}

#[cfg(feature = "metrics")]
#[gpui::test]
fn metrics_adapter_integration_with_mounted_program_verifies_cardinality_safety(
    cx: &mut TestAppContext,
) {
    use gpui_tea::{MetricsAdapter, observe_metrics_telemetry};

    #[derive(Clone, Debug, PartialEq, Eq)]
    enum Msg {
        Ping,
    }

    struct MetricModel;

    impl Model for MetricModel {
        type Msg = Msg;

        fn update(
            &mut self,
            _msg: Self::Msg,
            _cx: &mut App,
            _scope: &ModelContext<Self::Msg>,
        ) -> Command<Self::Msg> {
            Command::none()
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

    let has_label = |key: &metrics::Key, label_k: &str, label_v: &str| {
        key.labels()
            .any(|l| l.key() == label_k && l.value() == label_v)
    };
    let has_any_label =
        |key: &metrics::Key, label_k: &str| key.labels().any(|l| l.key() == label_k);

    // 1. Test with MetricsAdapter default (no program_id, stable program label from describe_program)
    let seen1 = Arc::new(Mutex::new(Vec::new()));
    let recorder1 = TestMetricRecorder {
        seen: seen1.clone(),
    };
    let adapter = MetricsAdapter::new();
    let config = ProgramConfig::default()
        .describe_program(|_| "stable_service")
        .telemetry_observer(adapter.observer());

    metrics::with_local_recorder(&recorder1, || {
        let program: Entity<Program<MetricModel>> =
            cx.update(|cx| MetricModel.into_program_with(config, cx));
        let dispatcher = program.read_with(cx, |program, _cx| program.dispatcher());

        dispatcher.dispatch(Msg::Ping).unwrap();
        cx.run_until_parked();

        // Release the program while this step's recorder is installed.
        drop(program);
        cx.update(|_cx| {});
    });

    {
        let keys = seen1.lock().unwrap();
        assert!(!keys.is_empty(), "metrics must have been recorded");
        assert!(
            keys.iter().all(|k| !has_any_label(k, "program_id")),
            "program_id must not be present by default"
        );
        assert!(
            keys.iter()
                .any(|k| has_label(k, "program", "stable_service")),
            "program label must match stable_service"
        );
    }

    // 2. Test with include_program_id explicitly enabled
    let seen2 = Arc::new(Mutex::new(Vec::new()));
    let recorder2 = TestMetricRecorder {
        seen: seen2.clone(),
    };
    let adapter2 = MetricsAdapter::new().include_program_id(true);
    let config2 = ProgramConfig::default()
        .describe_program(|_| "service_with_id")
        .telemetry_observer(adapter2.observer());

    metrics::with_local_recorder(&recorder2, || {
        let program: Entity<Program<MetricModel>> =
            cx.update(|cx| MetricModel.into_program_with(config2, cx));
        let dispatcher = program.read_with(cx, |program, _cx| program.dispatcher());

        dispatcher.dispatch(Msg::Ping).unwrap();
        cx.run_until_parked();

        // Release the program while this step's recorder is installed.
        drop(program);
        cx.update(|_cx| {});
    });

    {
        let keys = seen2.lock().unwrap();
        assert!(
            keys.iter().any(|k| has_any_label(k, "program_id")),
            "program_id must be present when enabled"
        );
        assert!(
            keys.iter()
                .any(|k| has_label(k, "program", "service_with_id"))
        );
    }

    // 3. Test with no program description and no configured name -> omits program label entirely
    let seen3 = Arc::new(Mutex::new(Vec::new()));
    let recorder3 = TestMetricRecorder {
        seen: seen3.clone(),
    };
    let adapter3 = MetricsAdapter::new();
    let config3 = ProgramConfig::default().telemetry_observer(adapter3.observer());

    metrics::with_local_recorder(&recorder3, || {
        let program: Entity<Program<MetricModel>> =
            cx.update(|cx| MetricModel.into_program_with(config3, cx));
        let dispatcher = program.read_with(cx, |program, _cx| program.dispatcher());

        dispatcher.dispatch(Msg::Ping).unwrap();
        cx.run_until_parked();

        // Release the program while this step's recorder is installed.
        drop(program);
        cx.update(|_cx| {});
    });

    {
        let keys = seen3.lock().unwrap();
        assert!(
            keys.iter().all(|k| !has_any_label(k, "program")),
            "program label must be omitted when not provided"
        );
        assert!(
            keys.iter().all(|k| !has_any_label(k, "program_id")),
            "program_id must not be present"
        );
    }

    // 4. Verify observe_metrics_telemetry function pointer records stable program label and omits program_id
    let seen4 = Arc::new(Mutex::new(Vec::new()));
    let recorder4 = TestMetricRecorder {
        seen: seen4.clone(),
    };
    let config_with_fn = ProgramConfig::default()
        .describe_program(|_| "stable_service_fn")
        .telemetry_observer(observe_metrics_telemetry);

    metrics::with_local_recorder(&recorder4, || {
        let program: Entity<Program<MetricModel>> =
            cx.update(|cx| MetricModel.into_program_with(config_with_fn, cx));
        let dispatcher = program.read_with(cx, |program, _cx| program.dispatcher());

        dispatcher.dispatch(Msg::Ping).unwrap();
        cx.run_until_parked();

        // Release the program while this step's recorder is installed.
        drop(program);
        cx.update(|_cx| {});
    });

    {
        let keys = seen4.lock().unwrap();
        assert!(
            keys.iter().all(|k| !has_any_label(k, "program_id")),
            "observe_metrics_telemetry must omit program_id"
        );
        assert!(
            keys.iter()
                .any(|k| has_label(k, "program", "stable_service_fn")),
            "observe_metrics_telemetry must include stable program label"
        );
    }
}

#[derive(Debug)]
enum BurstMsg {
    Tick,
}

struct BurstModel {
    processed: usize,
    subscription_calls: Arc<AtomicUsize>,
}

impl Model for BurstModel {
    type Msg = BurstMsg;

    fn update(
        &mut self,
        _msg: Self::Msg,
        _cx: &mut App,
        _scope: &ModelContext<Self::Msg>,
    ) -> Command<Self::Msg> {
        self.processed += 1;
        Command::none()
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

    fn subscriptions(
        &self,
        _cx: &mut App,
        _scope: &ModelContext<Self::Msg>,
    ) -> Subscriptions<Self::Msg> {
        self.subscription_calls.fetch_add(1, Ordering::SeqCst);
        Subscriptions::one(Subscription::new("burst", |_| SubHandle::None))
    }
}

#[gpui::test]
fn external_burst_is_drained_once_with_a_single_reconciliation(cx: &mut TestAppContext) {
    let drains = Arc::new(Mutex::new(Vec::new()));
    let subscription_calls = Arc::new(AtomicUsize::new(0));
    let config = ProgramConfig::default().observer({
        let drains = drains.clone();
        move |event: RuntimeEvent<'_, BurstMsg>| {
            if let RuntimeEvent::QueueDrainStarted { queued } = event {
                drains.lock().unwrap().push(queued);
            }
        }
    });
    let program = cx.update(|cx| {
        BurstModel {
            processed: 0,
            subscription_calls: subscription_calls.clone(),
        }
        .into_program_with(config, cx)
    });
    let dispatcher = program.read_with(cx, |program, _cx| program.dispatcher());
    let calls_after_mount = subscription_calls.load(Ordering::SeqCst);

    for _ in 0..100 {
        dispatcher.dispatch(BurstMsg::Tick).unwrap();
    }
    cx.run_until_parked();

    program.read_with(cx, |program, _cx| {
        assert_eq!(program.model().processed, 100);
        assert_eq!(program.runtime_snapshot().queue_depth, 0);
    });
    assert_eq!(drains.lock().unwrap().as_slice(), &[100]);
    assert_eq!(
        subscription_calls.load(Ordering::SeqCst) - calls_after_mount,
        1
    );
}

#[gpui::test]
fn queue_warning_and_snapshot_report_the_external_backlog(cx: &mut TestAppContext) {
    let warnings = Arc::new(Mutex::new(Vec::new()));
    let config = ProgramConfig::default()
        .queue_warning_threshold(2)
        .telemetry_observer({
            let warnings = warnings.clone();
            move |envelope: TelemetryEnvelope<'_, BurstMsg>| {
                if let TelemetryEvent::QueueWarning { queued, .. } = envelope.event {
                    warnings
                        .lock()
                        .unwrap()
                        .push((queued, envelope.metadata.queue_depth));
                }
            }
        });
    let program = cx.update(|cx| {
        BurstModel {
            processed: 0,
            subscription_calls: Arc::new(AtomicUsize::new(0)),
        }
        .into_program_with(config, cx)
    });
    let dispatcher = program.read_with(cx, |program, _cx| program.dispatcher());

    for _ in 0..5 {
        dispatcher.dispatch(BurstMsg::Tick).unwrap();
    }
    let backlog = program.read_with(cx, |program, _cx| program.runtime_snapshot().queue_depth);
    cx.run_until_parked();

    assert_eq!(backlog, 5);
    assert_eq!(
        warnings.lock().unwrap().as_slice(),
        &[(3, 3), (4, 4), (5, 5)]
    );
}

struct TrackedMsg(Arc<AtomicUsize>);

impl TrackedMsg {
    fn new(live: &Arc<AtomicUsize>) -> Self {
        live.fetch_add(1, Ordering::SeqCst);
        Self(live.clone())
    }
}

impl Drop for TrackedMsg {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::SeqCst);
    }
}

struct TrackedModel {
    processed: usize,
}

impl Model for TrackedModel {
    type Msg = TrackedMsg;

    fn update(
        &mut self,
        _msg: Self::Msg,
        _cx: &mut App,
        _scope: &ModelContext<Self::Msg>,
    ) -> Command<Self::Msg> {
        self.processed += 1;
        Command::none()
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

#[gpui::test]
fn drop_oldest_releases_displaced_messages_immediately(cx: &mut TestAppContext) {
    let live = Arc::new(AtomicUsize::new(0));
    let program = cx.update(|cx| {
        TrackedModel { processed: 0 }.into_program_with(
            ProgramConfig::default().queue_policy(QueuePolicy::DropOldest { capacity: 2 }),
            cx,
        )
    });
    let dispatcher = program.read_with(cx, |program, _cx| program.dispatcher());

    for _ in 0..1000 {
        dispatcher.dispatch(TrackedMsg::new(&live)).unwrap();
    }
    assert_eq!(live.load(Ordering::SeqCst), 2);

    cx.run_until_parked();
    assert_eq!(live.load(Ordering::SeqCst), 0);
    program.read_with(cx, |program, _cx| {
        assert_eq!(program.model().processed, 2);
    });
}

#[gpui::test]
fn dropping_the_program_rejects_dispatches_and_releases_queued_messages(cx: &mut TestAppContext) {
    let live = Arc::new(AtomicUsize::new(0));
    let unmounted = Arc::new(AtomicUsize::new(0));
    let config = ProgramConfig::default().telemetry_observer({
        let unmounted = unmounted.clone();
        move |envelope: TelemetryEnvelope<'_, TrackedMsg>| {
            if matches!(envelope.event, TelemetryEvent::ProgramUnmounted) {
                unmounted.fetch_add(1, Ordering::SeqCst);
            }
        }
    });
    let program = cx.update(|cx| TrackedModel { processed: 0 }.into_program_with(config, cx));
    let dispatcher = program.read_with(cx, |program, _cx| program.dispatcher());

    dispatcher.dispatch(TrackedMsg::new(&live)).unwrap();
    assert_eq!(live.load(Ordering::SeqCst), 1);

    drop(program);
    cx.update(|_cx| {});

    assert_eq!(unmounted.load(Ordering::SeqCst), 1);
    assert_eq!(live.load(Ordering::SeqCst), 0);
    assert_eq!(
        dispatcher.dispatch(TrackedMsg::new(&live)),
        Err(gpui_tea::Error::ProgramUnavailable)
    );
    assert_eq!(live.load(Ordering::SeqCst), 0);
}

#[derive(Debug)]
enum ChainMsg {
    Step(usize),
}

struct ChainModel {
    steps: Vec<usize>,
}

impl Model for ChainModel {
    type Msg = ChainMsg;

    fn init(&mut self, _cx: &mut App, _scope: &ModelContext<Self::Msg>) -> Command<Self::Msg> {
        Command::emit(ChainMsg::Step(1))
    }

    fn update(
        &mut self,
        msg: Self::Msg,
        _cx: &mut App,
        _scope: &ModelContext<Self::Msg>,
    ) -> Command<Self::Msg> {
        let ChainMsg::Step(step) = msg;
        self.steps.push(step);
        if step < 5 {
            Command::emit(ChainMsg::Step(step + 1))
        } else {
            Command::none()
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

#[gpui::test]
fn drain_budget_yields_between_chunks_and_preserves_order(cx: &mut TestAppContext) {
    let finished = Arc::new(Mutex::new(Vec::new()));
    let config = ProgramConfig::default().drain_budget(2).observer({
        let finished = finished.clone();
        move |event: RuntimeEvent<'_, ChainMsg>| {
            if let RuntimeEvent::QueueDrainFinished {
                processed,
                remaining,
            } = event
            {
                finished.lock().unwrap().push((processed, remaining));
            }
        }
    });
    let program = cx.update(|cx| ChainModel { steps: Vec::new() }.into_program_with(config, cx));

    program.read_with(cx, |program, _cx| {
        assert_eq!(program.model().steps, vec![1, 2]);
    });

    cx.run_until_parked();
    program.read_with(cx, |program, _cx| {
        assert_eq!(program.model().steps, vec![1, 2, 3, 4, 5]);
        assert_eq!(program.runtime_snapshot().queue_depth, 0);
    });
    assert_eq!(
        finished.lock().unwrap().as_slice(),
        &[(2, 1), (2, 1), (1, 0)]
    );
}
