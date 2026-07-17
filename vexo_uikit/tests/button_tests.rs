use std::sync::{
    atomic::{AtomicU32, Ordering},
    Arc,
};
use vexo_uikit::{Button, ButtonVariant, Platform, Widget};

#[test]
fn button_implements_widget() {
    let button = Button::new("Click me");
    let boxed: Box<dyn Widget> = button.boxed();
    assert!(boxed.as_any().downcast_ref::<Button>().is_some());
}

#[test]
fn button_default_variant_is_primary() {
    let button = Button::new("Test");
    assert_eq!(button.get_variant(), &ButtonVariant::Primary);
}

#[test]
fn button_builder_methods_work() {
    let button = Button::new("Delete")
        .variant(ButtonVariant::Destructive)
        .disabled(true)
        .platform(Platform::Mobile);
    assert_eq!(button.get_variant(), &ButtonVariant::Destructive);
    assert!(button.is_disabled());
}

#[test]
fn button_on_press_callback_fires() {
    let counter = Arc::new(AtomicU32::new(0));
    let counter_clone = counter.clone();
    let button = Button::new("Press").on_tap(move || {
        counter_clone.fetch_add(1, Ordering::SeqCst);
    });
    button.press();
    assert_eq!(counter.load(Ordering::SeqCst), 1);
}

#[test]
fn button_disabled_does_not_fire_callback() {
    let counter = Arc::new(AtomicU32::new(0));
    let counter_clone = counter.clone();
    let button = Button::new("Press").disabled(true).on_tap(move || {
        counter_clone.fetch_add(1, Ordering::SeqCst);
    });
    button.press();
    assert_eq!(counter.load(Ordering::SeqCst), 0);
}
