use std::sync::{Arc, Mutex};
use vexo::{ComponentState, Signal, State, StatefulMutable};

#[derive(ComponentState)]
struct TestState {
    count: StatefulMutable<u32>,
    label: String, // non-Signal field, should be skipped
}

impl Default for TestState {
    fn default() -> Self {
        Self {
            count: StatefulMutable::new(0),
            label: String::new(),
        }
    }
}

#[test]
fn derive_wires_signal_fields() {
    let mut state = TestState::default();
    let called = Arc::new(Mutex::new(false));
    let callback: Arc<dyn Fn() + Send + Sync> = {
        let called = called.clone();
        Arc::new(move || {
            *called.lock().unwrap() = true;
        })
    };
    state.set_dirty_callback(callback);

    // Setting the Signal field should trigger the callback
    state.count.set(1);
    assert!(*called.lock().unwrap());
}

#[test]
fn derive_skips_non_signal_fields() {
    // Just verify it compiles — non-Signal fields are skipped
    let mut state = TestState::default();
    let callback: Arc<dyn Fn() + Send + Sync> = Arc::new(|| {});
    state.set_dirty_callback(callback);
    // label is a String, not a Signal — no wiring needed
}

#[derive(ComponentState)]
struct TestStateWithSignalAlias {
    value: Signal<f64>,
}

impl Default for TestStateWithSignalAlias {
    fn default() -> Self {
        Self {
            value: Signal::new(0.0),
        }
    }
}

#[test]
fn derive_works_with_signal_alias() {
    let mut state = TestStateWithSignalAlias::default();
    let called = Arc::new(Mutex::new(false));
    let callback: Arc<dyn Fn() + Send + Sync> = {
        let called = called.clone();
        Arc::new(move || {
            *called.lock().unwrap() = true;
        })
    };
    state.set_dirty_callback(callback);

    state.value.set(42.0);
    assert!(*called.lock().unwrap());
}

#[derive(ComponentState)]
struct TestStateWithOption {
    optional_count: Option<StatefulMutable<u32>>,
    label: String,
}

impl Default for TestStateWithOption {
    fn default() -> Self {
        Self {
            optional_count: Some(StatefulMutable::new(0)),
            label: String::new(),
        }
    }
}

#[test]
fn derive_wires_option_signal_fields() {
    let mut state = TestStateWithOption::default();
    let called = Arc::new(Mutex::new(false));
    let callback: Arc<dyn Fn() + Send + Sync> = {
        let called = called.clone();
        Arc::new(move || {
            *called.lock().unwrap() = true;
        })
    };
    state.set_dirty_callback(callback);

    // Setting the Some(Signal) field should trigger the callback
    state.optional_count.as_ref().unwrap().set(5);
    assert!(*called.lock().unwrap());
}

#[test]
fn derive_handles_none_option_signal() {
    let mut state = TestStateWithOption {
        optional_count: None,
        label: String::new(),
    };
    let callback: Arc<dyn Fn() + Send + Sync> = Arc::new(|| {});
    // Should not panic when optional_count is None
    state.set_dirty_callback(callback);
}

#[derive(ComponentState)]
struct TestStateWithQualifiedPaths {
    value: vexo::Signal<i32>,
    name: String,
}

impl Default for TestStateWithQualifiedPaths {
    fn default() -> Self {
        Self {
            value: vexo::Signal::new(0),
            name: String::new(),
        }
    }
}

#[test]
fn derive_works_with_qualified_vexo_path() {
    let mut state = TestStateWithQualifiedPaths::default();
    let called = Arc::new(Mutex::new(false));
    let callback: Arc<dyn Fn() + Send + Sync> = {
        let called = called.clone();
        Arc::new(move || {
            *called.lock().unwrap() = true;
        })
    };
    state.set_dirty_callback(callback);

    state.value.set(99);
    assert!(*called.lock().unwrap());
}
