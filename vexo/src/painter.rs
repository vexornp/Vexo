//! Painter: generates render commands from the render object tree.
//!
//! This module extracts the paint-related logic from `ThreeTreePipeline`
//! into a standalone `Painter` struct for separation of concerns.

use crate::core::{Absolute, Bounds, Logical, Position, Relative};
use crate::render::RenderCommand;
use crate::style::Style;

use super::dirty::DirtyTracking;
use super::id::RenderObjectKey;
use super::render_object::{PaintContext, RenderObjectRegistry};

/// Paint decoration commands (background, border, corner radius, shadows)
/// for a `Style` at the given bounds.
///
/// This is the single source of truth for decoration painting — used by both
/// `ContainerRenderObject::paint()` and `DecoratedBoxRenderObject::paint()`.
/// The caller's `computed_bounds` provide the local bounds; the paint context's
/// `absolute_position()` provides the origin.
///
/// Note: `style.clip` is NOT handled here — it's exposed via
/// `RenderObject::clip_bounds()` and the painter pushes `PushClip`/`PopClip`
/// automatically around the RO's children.
pub(crate) fn paint_style(
    style: &Style,
    bounds: Bounds<Logical>,
    ctx: &mut PaintContext,
) -> Vec<RenderCommand> {
    let pos: Position<Logical, Absolute> = ctx.absolute_position();

    let absolute_bounds = Bounds::new(
        pos.x,
        pos.y,
        pos.x + bounds.width(),
        pos.y + bounds.height(),
    );

    let base_corner_radius = style
        .corner_radius
        .as_ref()
        .map(|cr| cr.radius)
        .unwrap_or(0.0);

    let mut commands = Vec::new();

    // 1. Emit shadows BEFORE fill/border (shadows draw behind everything).
    // Shadows bypass PushCornerRadius context — each shadow Rect carries its
    // own corner_radius field (computed as base + spread).
    // Shadows also bypass style.clip's PushClip — clipping the shadow to the
    // very shape casting it would make it invisible.
    for shadow in &style.shadows {
        if shadow.color.a == 0.0 {
            continue;
        }
        let blur = shadow.blur_radius.max(0.0);
        let pad = blur + shadow.spread_radius;
        let shadow_bounds = Bounds::new(
            absolute_bounds.left + shadow.offset.x - pad,
            absolute_bounds.top + shadow.offset.y - pad,
            absolute_bounds.right + shadow.offset.x + pad,
            absolute_bounds.bottom + shadow.offset.y + pad,
        );
        let shadow_corner_radius = (base_corner_radius + shadow.spread_radius).max(0.0);
        commands.push(RenderCommand::Rect {
            bounds: shadow_bounds,
            fill: shadow.color,
            stroke: None,
            corner_radius: shadow_corner_radius,
            shadow_color: shadow.color.to_array(),
            shadow_blur: blur,
        });
    }

    // 2. Push corner radius if set (affects fill/border only, NOT shadows)
    if let Some(ref cr) = style.corner_radius {
        commands.push(RenderCommand::PushCornerRadius { radius: cr.radius });
    }

    // 3. Draw background first (behind child)
    if let Some(bg_color) = style.background {
        commands.push(RenderCommand::rect(absolute_bounds, bg_color));
    }

    // 4. Draw border on top (after background)
    if let Some(ref border) = style.border {
        commands.push(RenderCommand::rect_with_border(
            absolute_bounds,
            crate::core::Color::TRANSPARENT,
            border.color,
            border.width,
        ));
    }

    // 5. Pop corner radius
    if style.corner_radius.is_some() {
        commands.push(RenderCommand::PopCornerRadius);
    }

    commands
}

/// Zero-sized struct that holds paint-related methods.
///
/// All methods are associated functions that take explicit parameters
/// instead of accessing `self` fields on a pipeline.
pub struct Painter;

impl Painter {
    /// Generate render commands from the render object tree.
    ///
    /// This method only paints objects that are marked as needing paint.
    /// It traverses from the root but only generates commands for dirty objects.
    pub fn paint(
        render_objects: &RenderObjectRegistry,
        dirty: &mut DirtyTracking,
    ) -> Vec<RenderCommand> {
        let dirty_paint_count = dirty.paint_count();
        let total_objects = render_objects.len();

        let mut commands = Vec::new();

        // If no root, nothing to paint
        let root_id = match render_objects.root() {
            Some(id) => id,
            None => return commands,
        };

        // Check if we need to paint
        if dirty.is_paint_empty() {
            log::debug!(
                "[RetainMode] paint() - No changes, regenerating commands for {} objects",
                total_objects
            );
        } else {
            log::debug!(
                "[RetainMode] paint() - Processing {} dirty objects out of {} total",
                dirty_paint_count,
                total_objects
            );
        }

        // Drain the dirty paint flags (we're about to paint them)
        let _dirty_ids: Vec<_> = dirty.drain_paint().collect();

        // Create paint context
        let mut ctx = PaintContext::new(&mut commands);

        // Paint root recursively (root starts at origin)
        Self::paint_recursive(render_objects, root_id, &mut ctx, Position::zero());

        log::debug!(
            "[RetainMode] paint() complete - generated {} render commands",
            commands.len()
        );

        commands
    }

    /// Recursively paint a render object and its children.
    pub(crate) fn paint_recursive(
        render_objects: &RenderObjectRegistry,
        id: RenderObjectKey,
        ctx: &mut PaintContext,
        parent_absolute_position: Position<Logical, Absolute>,
    ) {
        // Get the render object
        let obj = match render_objects.get(id) {
            Some(o) => o,
            None => return,
        };

        // Get this object's position relative to its parent (from Taffy layout)
        let position_in_parent: Position<Logical, Relative> = obj
            .computed_bounds()
            .map(|b| Position::new(b.left, b.top))
            .unwrap_or(Position::zero());

        // Calculate absolute position for this object:
        // parent's absolute position + this object's position within parent
        let absolute_position = position_in_parent.to_absolute(parent_absolute_position);

        // Tell the render object where to paint (in absolute coordinates)
        ctx.set_absolute_position(absolute_position);

        // Paint this object
        let local_commands = obj.paint(ctx);

        // Push commands from this object
        for cmd in local_commands {
            ctx.push_command(cmd);
        }

        // If this object has a paint transform, push it before painting children.
        // The origin is the center of this render object's bounds, so that
        // rotations happen around the center (matching the shader behavior).
        let transform = obj.paint_transform();
        if let Some(t) = &transform {
            let origin = obj
                .computed_bounds()
                .map(|b| {
                    crate::core::Point::new(
                        absolute_position.x + b.width() * 0.5,
                        absolute_position.y + b.height() * 0.5,
                    )
                })
                .unwrap_or(crate::core::Point::new(
                    absolute_position.x,
                    absolute_position.y,
                ));
            ctx.push_command(RenderCommand::PushTransform {
                transform: *t,
                origin,
            });
        }

        // If this object clips its children, push clip before painting children.
        let clip = obj.clip_bounds();
        if let Some(local_clip) = &clip {
            let absolute_clip = crate::core::Bounds::new(
                absolute_position.x,
                absolute_position.y,
                absolute_position.x + local_clip.width(),
                absolute_position.y + local_clip.height(),
            );
            ctx.push_command(RenderCommand::PushClip {
                bounds: absolute_clip,
            });
        }

        // If this object has a scroll offset, push it before painting children.
        let scroll = obj.scroll_offset();
        if let Some(offset) = &scroll {
            ctx.push_command(RenderCommand::PushOffset { offset: *offset });
        }

        // If this object has an opacity, push it before painting children.
        let opacity = obj.opacity();
        if let Some(opacity_value) = &opacity {
            ctx.push_command(RenderCommand::PushOpacity {
                opacity: *opacity_value,
            });
        }

        // Paint children
        //
        // Pass-through coordinate correction:
        // Pass-through ROs (ProxyRenderObject, Opacity, Offstage-onstage,
        // FractionalTranslation) share their child's Taffy node. Both the
        // pass-through RO and its child therefore read the *same*
        // `computed_bounds` (origin relative to the Taffy *grandparent*).
        // Without correction, the child's origin would be added a second
        // time — double-counting the shared offset and painting children at
        // the wrong position. Subtract `position_in_parent` so the child's
        // own `position_in_parent` (equal to this RO's, since they share the
        // Taffy node) cancels out.
        let child_parent_absolute = if obj.is_pass_through() {
            Position::new(
                absolute_position.x - position_in_parent.x,
                absolute_position.y - position_in_parent.y,
            )
        } else {
            absolute_position
        };
        for child_id in obj.children() {
            Self::paint_recursive(render_objects, *child_id, ctx, child_parent_absolute);
        }

        // Pop opacity after children
        if opacity.is_some() {
            ctx.push_command(RenderCommand::PopOpacity);
        }

        // Pop scroll offset after children
        if scroll.is_some() {
            ctx.push_command(RenderCommand::PopOffset);
        }

        // Pop clip after children
        if clip.is_some() {
            ctx.push_command(RenderCommand::PopClip);
        }

        // Pop transform after children
        if transform.is_some() {
            ctx.push_command(RenderCommand::PopTransform);
        }
    }
}
