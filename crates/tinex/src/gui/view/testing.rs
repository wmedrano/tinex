//! Headless Xilem 0.4 bridge. Its ViewCtx constructor/action map are private,
//! so a test-only transparent view records routes while Xilem builds the tree.
//! Refreshes obtain a context through the public application builder; no unsafe
//! access to Xilem internals or native event loop is needed.
use super::{AppState, UiAction};
use masonry::{
    core::{ErasedAction, Widget, WidgetId},
    testing::{TestHarness, TestHarnessParams},
    widgets::{Flex, SizedBox},
};
use std::{
    any::Any,
    cell::RefCell,
    collections::HashMap,
    rc::Rc,
    sync::{Arc, Mutex},
};
use xilem::core::{
    Environment, MessageContext, MessageResult, Mut, View, ViewId, ViewMarker, ViewPathTracker,
};
use xilem::{Pod, ViewCtx, WidgetView, WindowOptions, Xilem};

type RefreshCallback = Box<dyn FnOnce(&mut ViewCtx)>;

thread_local! {
    static ROUTES: RefCell<HashMap<WidgetId, Vec<ViewId>>> = RefCell::default();
    static BUILT: RefCell<Option<Box<dyn Any>>> = RefCell::default();
    static REFRESH: RefCell<Option<RefreshCallback>> = RefCell::default();
}

pub(super) struct Trace<V>(pub V);
impl<V> ViewMarker for Trace<V> {}
impl<V: WidgetView<AppState, UiAction>> View<AppState, UiAction, ViewCtx> for Trace<V> {
    type Element = V::Element;
    type ViewState = V::ViewState;
    fn build(&self, ctx: &mut ViewCtx, state: &mut AppState) -> (Self::Element, Self::ViewState) {
        let (pod, view_state) = self.0.build(ctx, state);
        ROUTES.with_borrow_mut(|map| {
            map.insert(pod.new_widget.id(), ctx.view_path().to_vec());
        });
        (pod, view_state)
    }
    fn rebuild(
        &self,
        prev: &Self,
        vs: &mut Self::ViewState,
        ctx: &mut ViewCtx,
        element: Mut<'_, Self::Element>,
        state: &mut AppState,
    ) {
        ROUTES.with_borrow_mut(|map| {
            map.insert(element.ctx.widget_id(), ctx.view_path().to_vec());
        });
        self.0.rebuild(&prev.0, vs, ctx, element, state);
    }
    fn teardown(
        &self,
        vs: &mut Self::ViewState,
        ctx: &mut ViewCtx,
        element: Mut<'_, Self::Element>,
    ) {
        ROUTES.with_borrow_mut(|map| {
            map.remove(&element.ctx.widget_id());
        });
        self.0.teardown(vs, ctx, element);
    }
    fn message(
        &self,
        vs: &mut Self::ViewState,
        msg: &mut MessageContext,
        element: Mut<'_, Self::Element>,
        state: &mut AppState,
    ) -> MessageResult<UiAction> {
        self.0.message(vs, msg, element, state)
    }
}

struct Probe<V> {
    view: Arc<V>,
    state: Arc<Mutex<AppState>>,
}
impl<V> ViewMarker for Probe<V> {}
impl<V: WidgetView<AppState, UiAction>> View<(), (), ViewCtx> for Probe<V> {
    type Element = V::Element;
    type ViewState = ();
    fn build(&self, ctx: &mut ViewCtx, _: &mut ()) -> (Self::Element, ()) {
        let (pod, vs) = self
            .view
            .as_ref()
            .build(ctx, &mut self.state.lock().unwrap());
        BUILT.with_borrow_mut(|slot| {
            *slot = Some(Box::new((
                vs,
                pod.new_widget.id(),
                ctx.view_path().to_vec(),
            )))
        });
        (pod, ())
    }
    fn rebuild(
        &self,
        _: &Self,
        _: &mut (),
        _: &mut ViewCtx,
        _: Mut<'_, Self::Element>,
        _: &mut (),
    ) {
    }
    fn teardown(&self, _: &mut (), _: &mut ViewCtx, _: Mut<'_, Self::Element>) {}
    fn message(
        &self,
        _: &mut (),
        _: &mut MessageContext,
        _: Mut<'_, Self::Element>,
        _: &mut (),
    ) -> MessageResult<()> {
        MessageResult::Nop
    }
}

struct Context;
impl ViewMarker for Context {}
impl View<(), (), ViewCtx> for Context {
    type Element = Pod<Flex>;
    type ViewState = ();
    fn build(&self, ctx: &mut ViewCtx, _: &mut ()) -> (Self::Element, ()) {
        REFRESH.with_borrow_mut(|slot| slot.take().unwrap())(ctx);
        (ctx.create_pod(Flex::column()), ())
    }
    fn rebuild(
        &self,
        _: &Self,
        _: &mut (),
        _: &mut ViewCtx,
        _: Mut<'_, Self::Element>,
        _: &mut (),
    ) {
    }
    fn teardown(&self, _: &mut (), _: &mut ViewCtx, _: Mut<'_, Self::Element>) {}
    fn message(
        &self,
        _: &mut (),
        _: &mut MessageContext,
        _: Mut<'_, Self::Element>,
        _: &mut (),
    ) -> MessageResult<()> {
        MessageResult::Nop
    }
}

pub(super) struct Session<V: WidgetView<AppState, UiAction>> {
    pub harness: TestHarness<SizedBox>,
    pub state: Arc<Mutex<AppState>>,
    view: Arc<V>,
    view_state: V::ViewState,
    root: WidgetId,
    base_path: Vec<ViewId>,
}

impl<V: WidgetView<AppState, UiAction>> Session<V> {
    pub fn new(
        state: AppState,
        make_view: impl FnOnce(&AppState) -> V,
        size: masonry::kurbo::Size,
        scale: f64,
    ) -> Rc<RefCell<Self>> {
        ROUTES.with_borrow_mut(HashMap::clear);
        let view = Arc::new(make_view(&state));
        let state = Arc::new(Mutex::new(state));
        let shared_view = view.clone();
        let shared_state = state.clone();
        let (_driver, mut windows) = Xilem::new_simple(
            (),
            move |_| Probe {
                view: shared_view.clone(),
                state: shared_state.clone(),
            },
            WindowOptions::new("Test"),
        )
        .into_driver_and_windows(Err);
        let (view_state, root, base_path) = *BUILT
            .with_borrow_mut(|slot| slot.take().unwrap())
            .downcast::<(V::ViewState, WidgetId, Vec<ViewId>)>()
            .unwrap();
        let mut params = TestHarnessParams::default();
        params.window_size = size;
        params.scale_factor = scale;
        params.background_color = super::super::theme::BACKGROUND;
        let harness = TestHarness::create_with(
            super::super::theme::properties(),
            SizedBox::new(windows.remove(0).root_widget)
                .expand()
                .with_auto_id(),
            params,
        );
        Rc::new(RefCell::new(Self {
            harness,
            state,
            view,
            view_state,
            root,
            base_path,
        }))
    }

    pub fn refresh(session: &Rc<RefCell<Self>>, make_view: impl FnOnce(&AppState) -> V) {
        let new_view = Arc::new(make_view(&session.borrow().state.lock().unwrap()));
        let session = session.clone();
        REFRESH.with_borrow_mut(|slot| {
            *slot = Some(Box::new(move |ctx| {
                let mut session = session.borrow_mut();
                assert_eq!(ctx.view_path(), session.base_path);
                let Self {
                    harness,
                    view,
                    view_state,
                    root,
                    state,
                    ..
                } = &mut *session;
                harness.edit_widget_with_id(*root, |mut widget| {
                    new_view.as_ref().rebuild(
                        view.as_ref(),
                        view_state,
                        ctx,
                        widget.downcast::<V::Widget>(),
                        &mut state.lock().unwrap(),
                    );
                });
                *view = new_view;
            }))
        });
        let _ = Xilem::new_simple((), |_| Context, WindowOptions::new("Context"))
            .into_driver_and_windows(Err);
    }

    pub fn path(&self, id: WidgetId) -> Vec<ViewId> {
        ROUTES.with_borrow(|routes| routes[&id][self.base_path.len()..].to_vec())
    }

    pub fn route(&mut self, id: WidgetId, action: ErasedAction) -> MessageResult<UiAction> {
        self.route_path(self.path(id), action)
    }

    pub fn route_path(
        &mut self,
        path: Vec<ViewId>,
        action: ErasedAction,
    ) -> MessageResult<UiAction> {
        let mut message =
            MessageContext::new(Environment::new(), path, xilem::core::DynMessage(action));
        let Self {
            harness,
            view,
            view_state,
            root,
            state,
            ..
        } = self;
        harness.edit_widget_with_id(*root, |mut widget| {
            view.as_ref().message(
                view_state,
                &mut message,
                widget.downcast::<V::Widget>(),
                &mut state.lock().unwrap(),
            )
        })
    }

    pub fn action(&mut self) -> Option<UiAction> {
        let (action, id) = self.harness.pop_action_erased()?;
        match self.route(id, action) {
            MessageResult::Action(action) => Some(action),
            _ => None,
        }
    }

    pub fn widgets<W: Widget + masonry::core::FromDynWidget>(&mut self) -> Vec<WidgetId> {
        let mut ids = Vec::new();
        self.harness.inspect_widgets(|widget| {
            if widget.downcast::<W>().is_some() {
                ids.push(widget.id());
            }
        });
        ids
    }

    pub fn button(&mut self, text: &str) -> WidgetId {
        self.widgets::<masonry::widgets::Button>()
            .into_iter()
            .find(|id| {
                self.harness.get_widget_with_id(*id).children()[0]
                    .downcast::<masonry::widgets::Label>()
                    .is_some_and(|label| label.inner().text().as_ref() == text)
            })
            .unwrap_or_else(|| panic!("missing button: {text}"))
    }

    pub fn labels(&mut self) -> Vec<String> {
        self.widgets::<masonry::widgets::Label>()
            .iter()
            .map(|id| {
                self.harness
                    .get_widget_with_id(*id)
                    .downcast::<masonry::widgets::Label>()
                    .unwrap()
                    .inner()
                    .text()
                    .to_string()
            })
            .collect()
    }
}
