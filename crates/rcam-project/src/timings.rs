//! Bounded numeric codec observations. No file paths, objects or runtime logging dependency.
use std::{cell::RefCell, collections::BTreeMap, time::Instant};
thread_local! { static VALUES: RefCell<BTreeMap<&'static str, u64>> = const { RefCell::new(BTreeMap::new()) }; }
pub fn take() -> BTreeMap<&'static str, u64> {
    VALUES.with(|values| std::mem::take(&mut *values.borrow_mut()))
}
pub(crate) fn add(key: &'static str, value: u64) {
    VALUES.with(|values| {
        let mut values = values.borrow_mut();
        let entry = values.entry(key).or_default();
        *entry = entry.saturating_add(value);
    });
}
pub(crate) struct Timer {
    key: &'static str,
    start: Instant,
}
impl Timer {
    pub fn new(key: &'static str) -> Self {
        Self {
            key,
            start: Instant::now(),
        }
    }
}
impl Drop for Timer {
    fn drop(&mut self) {
        add(
            self.key,
            self.start.elapsed().as_micros().min(u64::MAX as u128) as u64,
        );
    }
}
