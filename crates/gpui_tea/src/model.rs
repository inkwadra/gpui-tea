use crate::{Command, Dispatcher, ModelContext, Program, ProgramConfig, Subscriptions, View};
use gpui::{App, Entity, Window};

/// Define state, message handling, and rendering for a TEA program.
pub trait Model: Sized + 'static {
    /// Define the message type processed by this model.
    type Msg: Send + 'static;

    /// Initialize the model after it is mounted.
    ///
    /// Commands returned from `init()` obey the same queue-drain semantics as commands returned
    /// from [`Model::update()`].
    fn init(&mut self, _cx: &mut App, _scope: &ModelContext<Self::Msg>) -> Command<Self::Msg> {
        Command::none()
    }

    /// Apply `msg` to the model and return follow-up work.
    fn update(
        &mut self,
        msg: Self::Msg,
        cx: &mut App,
        scope: &ModelContext<Self::Msg>,
    ) -> Command<Self::Msg>;

    /// Render the current state of the model.
    fn view(
        &self,
        window: &mut Window,
        cx: &mut App,
        scope: &ModelContext<Self::Msg>,
        dispatcher: &Dispatcher<Self::Msg>,
    ) -> View;

    /// Describe the long-lived event sources required by this model.
    fn subscriptions(
        &self,
        _cx: &mut App,
        _scope: &ModelContext<Self::Msg>,
    ) -> Subscriptions<Self::Msg> {
        Subscriptions::none()
    }
}

/// Mount a [`Model`] as a fully initialized [`Program`].
pub trait ModelExt: Model {
    /// Mount this model as a fully initialized program entity.
    fn into_program(self, cx: &mut App) -> Entity<Program<Self>> {
        Program::mount(self, cx)
    }

    /// Mount this model with an explicit runtime configuration.
    fn into_program_with(
        self,
        config: ProgramConfig<Self::Msg>,
        cx: &mut App,
    ) -> Entity<Program<Self>> {
        Program::mount_with(self, config, cx)
    }
}

impl<M: Model> ModelExt for M {}
