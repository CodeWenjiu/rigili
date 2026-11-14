use std::sync::{Arc, Mutex, MutexGuard};

use super::status::{PlayerStatus, StatusSlot};

pub(super) fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

pub(super) fn mutate_status<T>(
    status: &StatusSlot,
    mutate: impl FnOnce(&mut PlayerStatus) -> T,
) -> T {
    let mut current = lock(status);
    let mut next = (**current).clone();
    let result = mutate(&mut next);
    *current = Arc::new(next);
    result
}
