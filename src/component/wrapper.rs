//! Wrapper for components with shared state.
use std::collections::HashSet;
use std::rc::Rc;

use yew::{
    agent::{Agent, AgentLink, Bridge, Bridged, Context, HandlerId},
    prelude::*,
};

use crate::handle::{SharedState, StateHandle, WrapperHandle};
use crate::handler::{Reduction, ReductionOnce, StateHandler};

pub enum Request<T> {
    /// Apply a state change.
    Apply(Reduction<T>),
    /// Apply a state change once.
    ApplyOnce(ReductionOnce<T>),
}

pub enum Response<T, S>
where
    S: Agent,
{
    /// Update subscribers with current state.
    State(Rc<T>),
    Link(AgentLink<S>),
}

/// Context agent for managing shared state. In charge of applying changes to state then notifying
/// subscribers of new state.
pub struct SharedStateService<HANDLER, SCOPE>
where
    HANDLER: StateHandler + 'static,
    SCOPE: 'static,
{
    handler: HANDLER,
    subscriptions: HashSet<HandlerId>,
    link: AgentLink<SharedStateService<HANDLER, SCOPE>>,
}

impl<HANDLER, SCOPE> Agent for SharedStateService<HANDLER, SCOPE>
where
    HANDLER: StateHandler + 'static,
    SCOPE: 'static,
{
    type Message = HANDLER::Message;
    type Reach = Context<Self>;
    type Input = Request<HANDLER::Model>;
    type Output = Response<HANDLER::Model, Self>;

    fn create(link: AgentLink<Self>) -> Self {
        Self {
            handler: <HANDLER as StateHandler>::new(),
            subscriptions: Default::default(),
            link,
        }
    }

    fn update(&mut self, msg: Self::Message) {
        let changed = self.handler.update(msg);
        if changed {
            self.handler.changed();
            self.notify_subscribers();
        }
    }

    fn handle_input(&mut self, msg: Self::Input, _who: HandlerId) {
        match msg {
            Request::Apply(reduce) => {
                reduce(Rc::make_mut(self.handler.state()));
                self.handler.changed();
            }
            Request::ApplyOnce(reduce) => {
                reduce(Rc::make_mut(self.handler.state()));
                self.handler.changed();
            }
        }

        self.notify_subscribers();
    }

    fn connected(&mut self, who: HandlerId) {
        // Add component to subscriptions.
        self.subscriptions.insert(who);
        // Send it current state.
        let state = Rc::clone(self.handler.state());
        self.link.respond(who, Response::State(state));
        self.link.respond(who, Response::Link(self.link.clone()));
    }

    fn disconnected(&mut self, who: HandlerId) {
        self.subscriptions.remove(&who);
    }
}

impl<HANDLER, SCOPE> SharedStateService<HANDLER, SCOPE>
where
    HANDLER: StateHandler + 'static,
    SCOPE: 'static,
{
    fn notify_subscribers(&mut self) {
        let state = self.handler.state();
        for who in self.subscriptions.iter().cloned() {
            self.link.respond(who, Response::State(Rc::clone(state)));
        }
    }
}

type PropHandle<SHARED> = <SHARED as SharedState>::Handle;
type PropHandler<SHARED> = <PropHandle<SHARED> as StateHandle>::Handler;
type Model<T> = <PropHandler<T> as StateHandler>::Model;

#[doc(hidden)]
pub enum SharedStateComponentMsg<SHARED>
where
    SHARED: SharedState,
    <SHARED as SharedState>::Handle: WrapperHandle,
    <<SHARED as SharedState>::Handle as StateHandle>::Scope: 'static,
    PropHandler<SHARED>: 'static,
{
    /// Recieve new local state.
    /// IMPORTANT: Changes will **not** be reflected in shared state.
    SetLocal(Rc<Model<SHARED>>),
    SetLink(
        AgentLink<
            SharedStateService<
                PropHandler<SHARED>,
                <<SHARED as SharedState>::Handle as StateHandle>::Scope,
            >,
        >,
    ),
    /// Update shared state.
    Apply(Reduction<Model<SHARED>>),
    ApplyOnce(ReductionOnce<Model<SHARED>>),
}

/// Component wrapper for managing messages and state handles.
///
/// Wraps any component with properties that implement `SharedState`:
/// ```
/// pub type MyComponent = SharedStateComponent<MyComponentModel>;
/// ```
///
/// A scope may be provided to specify where the state is shared:
/// ```
/// // This will only share state with other components using `FooScope`.
/// pub struct FooScope;
/// pub type MyComponent = SharedStateComponent<MyComponentModel, FooScope>;
/// ```
///
/// # Important
/// By default `StorageHandle` and `GlobalHandle` have different scopes. Though not enforced,
/// components with different handles should not use the same scope.
pub struct SharedStateComponent<C>
where
    C: Component,
    C::Properties: SharedState + Clone,
    PropHandle<C::Properties>: WrapperHandle,
    <PropHandle<C::Properties> as StateHandle>::Scope: 'static,
{
    props: C::Properties,
    bridge: Box<
        dyn Bridge<
            SharedStateService<
                PropHandler<C::Properties>,
                <PropHandle<C::Properties> as StateHandle>::Scope,
            >,
        >,
    >,
    link_set: bool,
    state_set: bool,
}

impl<C> Component for SharedStateComponent<C>
where
    C: Component,
    C::Properties: SharedState + Clone,
    <C::Properties as SharedState>::Handle: Clone + WrapperHandle,
{
    type Message = SharedStateComponentMsg<C::Properties>;
    type Properties = C::Properties;

    fn create(mut props: Self::Properties, link: ComponentLink<Self>) -> Self {
        use SharedStateComponentMsg::*;
        // Bridge to receive new state.
        let callback = link.callback(|msg| match msg {
            Response::State(state) => SetLocal(state),
            Response::Link(link) => SetLink(link),
        });
        let bridge = SharedStateService::bridge(callback);

        props
            .handle()
            .set_callbacks(link.callback(Apply), link.callback(ApplyOnce));

        SharedStateComponent {
            props,
            bridge,
            state_set: Default::default(),
            link_set: Default::default(),
        }
    }

    fn update(&mut self, msg: Self::Message) -> ShouldRender {
        use SharedStateComponentMsg::*;
        match msg {
            Apply(reduce) => {
                self.bridge.send(Request::Apply(reduce));
                false
            }
            ApplyOnce(reduce) => {
                self.bridge.send(Request::ApplyOnce(reduce));
                false
            }
            SetLocal(state) => {
                self.props.handle().set_state(state);
                self.state_set = true;
                true
            }
            SetLink(link) => {
                self.props.handle().set_link(link);
                self.link_set = true;
                true
            }
        }
    }

    fn change(&mut self, mut props: Self::Properties) -> ShouldRender {
        *props.handle() = props.handle().clone();
        self.props = props;
        true
    }

    fn view(&self) -> Html {
        if self.link_set && self.link_set {
            let props = self.props.clone();
            html! {
                <C with props />
            }
        } else {
            html! {}
        }
    }
}
