use vexo::inherited_registry::{InheritedMap, InheritedRegistry};
use vexo::layout::AlignSelf;
use vexo::{BuildOwner, ElementKey, RenderContext, ThemeData};
use vexo::{DecoratedBox, Opacity, Text, Widget, WithLayout};
use vexo_uikit::theme::tokens;
use vexo_uikit::{Button, ButtonState, ButtonVariant, Component, Platform};

fn make_element_key() -> ElementKey {
    let mut sm: slotmap::SlotMap<ElementKey, ()> = slotmap::SlotMap::with_key();
    sm.insert(())
}

fn create_render_context<'a>(
    element_id: ElementKey,
    build_owner: &'a BuildOwner,
    inherited_map: &'a InheritedMap,
    inherited_registry: &'a InheritedRegistry,
) -> RenderContext<'a> {
    RenderContext::new(element_id, build_owner, inherited_map, inherited_registry)
}

/// Render a Button and return the widget tree, with a throwaway RenderContext.
fn render_button(button: Button) -> Box<dyn Widget> {
    let mut state = ButtonState::default();
    let element_id = make_element_key();
    let build_owner = BuildOwner::new();
    let inherited_map = InheritedMap::empty();
    let inherited_registry = InheritedRegistry::new();
    let mut ctx = create_render_context(
        element_id,
        &build_owner,
        &inherited_map,
        &inherited_registry,
    );
    button.render(&mut state, &mut ctx)
}

/// Walk down a single-child widget chain by repeatedly calling
/// `Widget::child()`, returning the first node that downcasts to `T`.
/// Stops after `max_depth` hops to avoid infinite loops on malformed trees.
fn find_in_chain<'a, T: 'static>(mut w: &'a dyn Widget, max_depth: usize) -> Option<&'a T> {
    for _ in 0..max_depth {
        if let Some(found) = w.as_any().downcast_ref::<T>() {
            return Some(found);
        }
        match w.child() {
            Some(c) => w = c,
            None => return None,
        }
    }
    w.as_any().downcast_ref::<T>()
}

#[test]
fn button_primary_render_does_not_panic() {
    let button = Button::new("Click").variant(ButtonVariant::Primary);
    let _widget = render_button(button);
}

#[test]
fn button_disabled_render_does_not_panic() {
    let button = Button::new("Save").disabled(true);
    let _widget = render_button(button);
}

#[test]
fn button_hover_state_render_does_not_panic() {
    let button = Button::new("Hover")
        .variant(ButtonVariant::Primary)
        .platform(Platform::Desktop);

    // Render without hover
    let mut state = ButtonState::default();
    let element_id = make_element_key();
    let build_owner = BuildOwner::new();
    let inherited_map = InheritedMap::empty();
    let inherited_registry = InheritedRegistry::new();
    let mut ctx = create_render_context(
        element_id,
        &build_owner,
        &inherited_map,
        &inherited_registry,
    );
    let _unhovered = button.render(&mut state, &mut ctx);

    // Simulate hover
    state.is_hovered.set(true);

    // Render with hover
    let mut ctx2 = create_render_context(
        element_id,
        &build_owner,
        &inherited_map,
        &inherited_registry,
    );
    let _hovered = button.render(&mut state, &mut ctx2);
}

#[test]
fn button_all_variants_render() {
    for variant in [
        ButtonVariant::Primary,
        ButtonVariant::Secondary,
        ButtonVariant::Destructive,
        ButtonVariant::Ghost,
    ] {
        let button = Button::new("Test").variant(variant);
        let _widget = render_button(button);
    }
}

// ============================================================================
// Structural tests — verify the widget tree shape after the intrinsic-sizing
// fix. Decoration must live on DecoratedBox (not on the Text leaf),
// and align_self(Start) must be the outermost modifier to break the parent
// Column's AlignItems::Stretch cascade.
// ============================================================================

#[test]
fn button_outermost_has_align_self_start() {
    let tree = render_button(Button::new("Submit").variant(ButtonVariant::Primary));

    // The outermost widget must be WithLayout with align_self=Start.
    // This breaks the Column's AlignItems::Stretch cascade so the whole
    // subtree (including pass-through wrappers) sizes to the button's
    // intrinsic width.
    let outer = tree
        .as_any()
        .downcast_ref::<WithLayout>()
        .expect("outermost widget should be WithLayout(align_self=Start)");
    assert_eq!(
        outer.layout_ref().align_self,
        Some(AlignSelf::Start),
        "outermost WithLayout must set align_self=Start to break Column stretch"
    );
}

#[test]
fn button_decoration_on_container_not_text() {
    let tree = render_button(Button::new("Submit").variant(ButtonVariant::Primary));

    // Find the DecoratedBox anywhere in the child chain. The
    // pass-through wrappers (GestureDetector, MouseRegion) between the
    // outermost WithLayout and the DecoratedBox are pub(crate) in
    // vexo, so we walk generically rather than asserting each layer.
    let db = find_in_chain::<DecoratedBox>(tree.as_ref(), 8)
        .expect("expected a DecoratedBox carrying the visual decoration");

    // Background must be on the container, not on the Text leaf.
    assert_eq!(
        db.style_ref().background,
        Some(ThemeData::light().primary),
        "background should live on DecoratedBox, not Text"
    );

    // Padding lives on the WithLayout inside DecoratedBox.
    // Button::resolve_padding returns (PADDING_V, PADDING_H, PADDING_V, PADDING_H)
    // in TRBL order; padding_each(top, right, bottom, left) delegates to
    // Layout::padding_each(left, right, top, bottom).
    let wl = db
        .child()
        .as_any()
        .downcast_ref::<WithLayout>()
        .expect("DecoratedBox's child should be WithLayout with padding");
    let padding = wl
        .layout_ref()
        .padding
        .expect("WithLayout should have padding");
    assert_eq!(padding.top, tokens::button::PADDING_V_DESKTOP);
    assert_eq!(padding.bottom, tokens::button::PADDING_V_DESKTOP);
    assert_eq!(padding.left, tokens::button::PADDING_H_DESKTOP);
    assert_eq!(padding.right, tokens::button::PADDING_H_DESKTOP);
}

#[test]
fn button_text_is_pure_leaf() {
    let tree = render_button(Button::new("Submit").variant(ButtonVariant::Primary));

    let db = find_in_chain::<DecoratedBox>(tree.as_ref(), 8).expect("expected a DecoratedBox");

    // The DecoratedBox's child is WithLayout; its child must be a Text leaf.
    let wl = db
        .child()
        .as_any()
        .downcast_ref::<WithLayout>()
        .expect("DecoratedBox's child should be WithLayout");
    let text = wl
        .child()
        .expect("WithLayout should have a child")
        .as_any()
        .downcast_ref::<Text>()
        .expect("WithLayout's child should be a Text leaf");
    assert_eq!(text.content(), "Submit");

    // And Text must be a true leaf (no further children).
    assert!(
        text.child().is_none(),
        "Text should be a pure leaf with no children"
    );
}

#[test]
fn button_secondary_has_border_on_container() {
    let tree = render_button(Button::new("Cancel").variant(ButtonVariant::Secondary));

    let dc = find_in_chain::<DecoratedBox>(tree.as_ref(), 8).expect("expected a DecoratedBox");

    // Secondary variant has a 1px border on the container.
    let border = dc
        .style_ref()
        .border
        .as_ref()
        .expect("Secondary should have a border on DecoratedBox");
    assert_eq!(border.color, ThemeData::light().outline);
    assert_eq!(border.width, 1.0);

    // Background is transparent for Secondary.
    assert_eq!(dc.style_ref().background, Some(vexo::Color::TRANSPARENT));
}

#[test]
fn button_disabled_applies_opacity() {
    let tree = render_button(
        Button::new("Submit")
            .variant(ButtonVariant::Primary)
            .disabled(true),
    );

    // The Opacity wrapper must carry the disabled opacity token.
    let op = find_in_chain::<Opacity>(tree.as_ref(), 8)
        .expect("expected an Opacity layer for disabled state");
    assert_eq!(op.opacity_value(), tokens::button::DISABLED_OPACITY);
}

// ============================================================================
// Text color tests — verify each variant applies its token color to the
// Text leaf via Text::with_color.
// ============================================================================

/// Extract the Text leaf from a rendered Button tree.
fn text_leaf_from_tree(tree: &Box<dyn Widget>) -> &Text {
    let db = find_in_chain::<DecoratedBox>(tree.as_ref(), 8).expect("expected a DecoratedBox");
    let wl = db
        .child()
        .as_any()
        .downcast_ref::<WithLayout>()
        .expect("DecoratedBox's child should be WithLayout");
    wl.child()
        .expect("WithLayout should have a child")
        .as_any()
        .downcast_ref::<Text>()
        .expect("WithLayout's child should be a Text leaf")
}

#[test]
fn button_primary_text_color_matches_token() {
    let tree = render_button(Button::new("Submit").variant(ButtonVariant::Primary));
    let text = text_leaf_from_tree(&tree);
    assert_eq!(text.color(), ThemeData::light().on_primary);
}

#[test]
fn button_secondary_text_color_matches_token() {
    let tree = render_button(Button::new("Cancel").variant(ButtonVariant::Secondary));
    let text = text_leaf_from_tree(&tree);
    assert_eq!(text.color(), ThemeData::light().primary);
}

#[test]
fn button_destructive_text_color_matches_token() {
    let tree = render_button(Button::new("Delete").variant(ButtonVariant::Destructive));
    let text = text_leaf_from_tree(&tree);
    assert_eq!(text.color(), ThemeData::light().on_error);
}

#[test]
fn button_ghost_text_color_matches_token() {
    let tree = render_button(Button::new("More").variant(ButtonVariant::Ghost));
    let text = text_leaf_from_tree(&tree);
    assert_eq!(text.color(), ThemeData::light().primary);
}

#[test]
fn button_ghost_hover_uses_hover_text_color() {
    // Simulate hover by rendering with is_hovered=true. We can't use
    // render_button (it uses a fresh state), so render manually like the
    // hover_state_render_does_not_panic test does.
    let button = Button::new("More")
        .variant(ButtonVariant::Ghost)
        .platform(Platform::Desktop);
    let mut state = ButtonState::default();
    state.is_hovered.set(true);
    let element_id = make_element_key();
    let build_owner = BuildOwner::new();
    let inherited_map = InheritedMap::empty();
    let inherited_registry = InheritedRegistry::new();
    let mut ctx = create_render_context(
        element_id,
        &build_owner,
        &inherited_map,
        &inherited_registry,
    );
    let tree = button.render(&mut state, &mut ctx);

    let text = text_leaf_from_tree(&tree);
    assert_eq!(
        text.color(),
        vexo::Color::lerp(ThemeData::light().primary, vexo::Color::WHITE, 0.15)
    );
}
