use std::{
    fmt,
    hash::{Hash, Hasher},
    marker::PhantomData,
    sync::atomic::{AtomicU64, Ordering},
};

/// A unique, process-local identifier tagged with its associated type.
pub struct Id<T> {
    value: u64,
    marker: PhantomData<fn() -> T>,
}

impl<T> Id<T> {
    pub fn new() -> Self {
        static NEXT_ID: AtomicU64 = AtomicU64::new(0);

        let value = NEXT_ID
            .try_update(Ordering::Relaxed, Ordering::Relaxed, |value| {
                value.checked_add(1)
            })
            .expect("ID counter exhausted");
        Self {
            value,
            marker: PhantomData,
        }
    }
}

impl<T> Default for Id<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T> Copy for Id<T> {}

impl<T> Clone for Id<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T> fmt::Debug for Id<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let type_name = std::any::type_name::<T>();
        let type_name = type_name.rsplit("::").next().unwrap_or(type_name);
        f.write_str(type_name)?;
        f.debug_tuple("Id").field(&self.value).finish()
    }
}

impl<T> Hash for Id<T> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.value.hash(state);
    }
}

impl<T> PartialEq for Id<T> {
    fn eq(&self, other: &Self) -> bool {
        self.value == other.value
    }
}

impl<T> Eq for Id<T> {}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    // IDs implement their traits even when the associated type does not.
    struct Marker;

    #[test]
    fn constructors_generate_unique_ids_across_threads() {
        let threads: Vec<_> = (0..4)
            .map(|_| {
                std::thread::spawn(|| {
                    (0..100)
                        .flat_map(|_| [Id::<Marker>::new(), Id::default()])
                        .collect::<Vec<_>>()
                })
            })
            .collect();
        let ids: HashSet<_> = threads
            .into_iter()
            .flat_map(|thread| thread.join().unwrap())
            .collect();
        assert_eq!(ids.len(), 800);
        let id = *ids.iter().next().unwrap();
        let copied = id;
        assert_eq!(id, copied);
    }
}
