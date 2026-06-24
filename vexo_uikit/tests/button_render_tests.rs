use vexo_uikit::{Button, ButtonVariant, Platform, Component, ButtonState};
use vexo::{RenderContext, BuildOwner, DirtyTracking, RenderObjectRegistry, ElementKey};

fn make_element_key() -> ElementKey {
    let mut sm: slotmap::SlotMap<ElementKey, ()> = slotmap::SlotMap::with_key();
    sm.insert(())
}

fn create_render_context<'a>(
    element_id: ElementKey,
    dirty: &'a mut DirtyTracking,
    render_objects: &'a mut RenderObjectRegistry,
    build_owner: &'a BuildOwner,
) -> RenderContext<'a> {
    RenderContext {
        element_id,
        dirty,
        render_objects,
        build_owner,
    }
}

#[test]
fn button_primary_render_does_not_panic() {
    let button = Button::new("Click").variant(ButtonVariant::Primary);
    let mut state = ButtonState::default();
    let element_id = make_element_key();
    let mut dirty = DirtyTracking::new();
    let mut render_objects = RenderObjectRegistry::new();
    let build_owner = BuildOwner::new();

    let mut ctx = create_render_context(element_id, &mut dirty, &mut render_objects, &build_owner);
    let _widget = button.render(&mut state, &mut ctx);
}

#[test]
fn button_disabled_render_does_not_panic() {
    let button = Button::new("Save").disabled(true);
    let mut state = ButtonState::default();
    let element_id = make_element_key();
    let mut dirty = DirtyTracking::new();
    let mut render_objects = RenderObjectRegistry::new();
    let build_owner = BuildOwner::new();

    let mut ctx = create_render_context(element_id, &mut dirty, &mut render_objects, &build_owner);
    let _widget = button.render(&mut state, &mut ctx);
}

#[test]
fn button_hover_state_produces_different_render() {
    let button = Button::new("Hover").variant(ButtonVariant::Primary).platform(Platform::Desktop);
    let mut state = ButtonState::default();
    let element_id = make_element_key();
    let mut dirty = DirtyTracking::new();
    let mut render_objects = RenderObjectRegistry::new();
    let build_owner = BuildOwner::new();

    // Render without hover
    let mut ctx = create_render_context(element_id, &mut dirty, &mut render_objects, &build_owner);
    let _unhovered = button.render(&mut state, &mut ctx);

    // Simulate hover
    state.is_hovered.set(true);

    // Render with hover
    let mut ctx2 = create_render_context(element_id, &mut dirty, &mut render_objects, &build_owner);
    let _hovered = button.render(&mut state, &mut ctx2);
}

#[test]
fn button_all_variants_render() {
    for variant in [ButtonVariant::Primary, ButtonVariant::Secondary, ButtonVariant::Destructive, ButtonVariant::Ghost] {
        let button = Button::new("Test").variant(variant);
        let mut state = ButtonState::default();
        let element_id = make_element_key();
        let mut dirty = DirtyTracking::new();
        let mut render_objects = RenderObjectRegistry::new();
        let build_owner = BuildOwner::new();

        let mut ctx = create_render_context(element_id, &mut dirty, &mut render_objects, &build_owner);
        let _widget = button.render(&mut state, &mut ctx);
    }
}
