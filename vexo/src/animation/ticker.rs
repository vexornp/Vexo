use std::cell::RefCell;
use std::sync::Arc;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TickHandle(usize);

pub struct AnimationTicker {
    active: RefCell<Vec<(TickHandle, Arc<dyn Fn() + Send + Sync>)>>,
    next_id: RefCell<usize>,
}

impl AnimationTicker {
    pub fn new() -> Self {
        Self {
            active: RefCell::new(Vec::new()),
            next_id: RefCell::new(0),
        }
    }

    pub fn register(&self, cb: Arc<dyn Fn() + Send + Sync>) -> TickHandle {
        let mut id = self.next_id.borrow_mut();
        let handle = TickHandle(*id);
        *id += 1;
        self.active.borrow_mut().push((handle, cb));
        handle
    }

    pub fn unregister(&self, handle: TickHandle) {
        self.active.borrow_mut().retain(|(h, _)| *h != handle);
    }

    pub fn tick(&self) {
        let active = self.active.borrow();
        for (_, cb) in active.iter() {
            cb();
        }
    }

    pub fn has_active(&self) -> bool {
        !self.active.borrow().is_empty()
    }
}

impl Default for AnimationTicker {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[test]
    fn test_ticker_new_is_empty() {
        let ticker = AnimationTicker::new();
        assert!(!ticker.has_active());
    }

    #[test]
    fn test_ticker_register_makes_active() {
        let ticker = AnimationTicker::new();
        let cb: Arc<dyn Fn() + Send + Sync> = Arc::new(|| {});
        ticker.register(cb);
        assert!(ticker.has_active());
    }

    #[test]
    fn test_ticker_tick_fires_callbacks() {
        let ticker = AnimationTicker::new();
        let counter = Arc::new(AtomicUsize::new(0));
        let counter_clone = counter.clone();
        let cb: Arc<dyn Fn() + Send + Sync> = Arc::new(move || {
            counter_clone.fetch_add(1, Ordering::SeqCst);
        });
        ticker.register(cb);
        ticker.tick();
        assert_eq!(counter.load(Ordering::SeqCst), 1);
        ticker.tick();
        assert_eq!(counter.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn test_ticker_unregister_removes_callback() {
        let ticker = AnimationTicker::new();
        let counter = Arc::new(AtomicUsize::new(0));
        let counter_clone = counter.clone();
        let cb: Arc<dyn Fn() + Send + Sync> = Arc::new(move || {
            counter_clone.fetch_add(1, Ordering::SeqCst);
        });
        let handle = ticker.register(cb);
        ticker.tick();
        assert_eq!(counter.load(Ordering::SeqCst), 1);
        ticker.unregister(handle);
        assert!(!ticker.has_active());
        ticker.tick();
        assert_eq!(counter.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn test_ticker_multiple_callbacks() {
        let ticker = AnimationTicker::new();
        let counter = Arc::new(AtomicUsize::new(0));
        let cb1: Arc<dyn Fn() + Send + Sync> = {
            let c = counter.clone();
            Arc::new(move || { c.fetch_add(1, Ordering::SeqCst); })
        };
        let cb2: Arc<dyn Fn() + Send + Sync> = {
            let c = counter.clone();
            Arc::new(move || { c.fetch_add(10, Ordering::SeqCst); })
        };
        ticker.register(cb1);
        ticker.register(cb2);
        ticker.tick();
        assert_eq!(counter.load(Ordering::SeqCst), 11);
    }
}
