//! Keyed, single-element flex sequences for Xilem 0.4 (whose Vec is positional).
use xilem::ViewCtx;
use xilem::core::{
    AppendVec, Count, ElementSplice, MessageContext, MessageResult, ViewId, ViewPathTracker,
    ViewSequence,
};
use xilem::view::FlexElement;

/// Keep existing item identities across insertions and deletions.
///
/// Each item must produce exactly one flex element and keys must be unique.
/// Relative order is stable in Tinex's track/plugin chains. For an actual reorder,
/// moved items are rebuilt because Masonry 0.4 has no child-reordering API; their
/// obsolete messages are rejected rather than delivered to a different item.
pub fn keyed<K, V>(items: impl IntoIterator<Item = (K, V)>) -> Keyed<K, V> {
    Keyed {
        items: items.into_iter().collect(),
    }
}

pub struct Keyed<K, V> {
    items: Vec<(K, V)>,
}
pub struct KeyedState<K, S> {
    entries: Vec<Entry<K, S>>,
    next_id: u64,
}
struct Entry<K, S> {
    key: K,
    id: ViewId,
    state: S,
}

impl<K, V, State, Action> ViewSequence<State, Action, ViewCtx, FlexElement> for Keyed<K, V>
where
    K: Clone + Eq + 'static,
    V: ViewSequence<State, Action, ViewCtx, FlexElement>,
{
    type SeqState = KeyedState<K, V::SeqState>;
    const ELEMENTS_COUNT: Count = Count::Many;

    fn seq_build(
        &self,
        ctx: &mut ViewCtx,
        elements: &mut AppendVec<FlexElement>,
        app: &mut State,
    ) -> Self::SeqState {
        let mut state = KeyedState {
            entries: Vec::new(),
            next_id: 0,
        };
        for (key, view) in &self.items {
            assert!(
                !state.entries.iter().any(|e| &e.key == key),
                "duplicate keyed view"
            );
            let id = ViewId::new(state.next_id);
            state.next_id += 1;
            let before = elements.index();
            let child_state = ctx.with_id(id, |ctx| view.seq_build(ctx, elements, app));
            assert_eq!(
                elements.index() - before,
                1,
                "keyed requires single-element views"
            );
            state.entries.push(Entry {
                key: key.clone(),
                id,
                state: child_state,
            });
        }
        state
    }

    fn seq_rebuild(
        &self,
        prev: &Self,
        state: &mut Self::SeqState,
        ctx: &mut ViewCtx,
        elements: &mut impl ElementSplice<FlexElement>,
        app: &mut State,
    ) {
        for (i, (key, _)) in self.items.iter().enumerate() {
            assert!(
                !self.items[..i].iter().any(|(k, _)| k == key),
                "duplicate keyed view"
            );
        }
        // Retain an ordered subsequence of old children. Insertions/deletions
        // preserve every surviving identity; genuine moves get new generations.
        let mut keep = Vec::new();
        let mut start = 0;
        for (key, _) in &self.items {
            if let Some(offset) = state.entries[start..].iter().position(|e| &e.key == key) {
                let index = start + offset;
                keep.push(state.entries[index].key.clone());
                start = index + 1;
            }
        }
        let mut retained = Vec::new();
        // A splice cannot rewind, so remove old children and rebuild retained
        // children together in a single forward pass, inserting new children
        // before each survivor as needed.
        let mut next_new = 0;
        for mut entry in std::mem::take(&mut state.entries) {
            let old_view = &prev.items.iter().find(|(k, _)| k == &entry.key).unwrap().1;
            if !keep.contains(&entry.key) {
                ctx.with_id(entry.id, |ctx| {
                    old_view.seq_teardown(&mut entry.state, ctx, elements)
                });
                continue;
            }
            while self.items[next_new].0 != entry.key {
                let (key, view) = &self.items[next_new];
                let id = ViewId::new(state.next_id);
                state.next_id = state.next_id.checked_add(1).expect("view id exhausted");
                let child_state = ctx.with_id(id, |ctx| {
                    elements.with_scratch(|scratch| view.seq_build(ctx, scratch, app))
                });
                retained.push(Entry {
                    key: key.clone(),
                    id,
                    state: child_state,
                });
                next_new += 1;
            }
            let view = &self.items[next_new].1;
            ctx.with_id(entry.id, |ctx| {
                view.seq_rebuild(old_view, &mut entry.state, ctx, elements, app)
            });
            retained.push(entry);
            next_new += 1;
        }
        for (key, view) in &self.items[next_new..] {
            let id = ViewId::new(state.next_id);
            state.next_id = state.next_id.checked_add(1).expect("view id exhausted");
            let child_state = ctx.with_id(id, |ctx| {
                elements.with_scratch(|scratch| view.seq_build(ctx, scratch, app))
            });
            retained.push(Entry {
                key: key.clone(),
                id,
                state: child_state,
            });
        }
        state.entries = retained;
    }

    fn seq_teardown(
        &self,
        state: &mut Self::SeqState,
        ctx: &mut ViewCtx,
        elements: &mut impl ElementSplice<FlexElement>,
    ) {
        for ((_, view), entry) in self.items.iter().zip(&mut state.entries) {
            ctx.with_id(entry.id, |ctx| {
                view.seq_teardown(&mut entry.state, ctx, elements)
            });
        }
    }

    fn seq_message(
        &self,
        state: &mut Self::SeqState,
        message: &mut MessageContext,
        elements: &mut impl ElementSplice<FlexElement>,
        app: &mut State,
    ) -> MessageResult<Action> {
        let Some(id) = message.take_first() else {
            return MessageResult::Stale;
        };
        let Some(index) = state.entries.iter().position(|entry| entry.id == id) else {
            return MessageResult::Stale;
        };
        elements.skip(index);
        self.items[index]
            .1
            .seq_message(&mut state.entries[index].state, message, elements, app)
    }
}
