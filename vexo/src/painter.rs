//! Painter: generates render commands from the render object tree.
//!
//! This module extracts the paint-related logic from `ThreeTreePipeline`
//! into a standalone `Painter` struct for separation of concerns.

use crate::core::{Absolute, Logical, Position, Relative};
use crate::render::RenderCommand;

use super::dirty::DirtyTracking;
use super::id::RenderObjectKey;
use super::render_object::{PaintContext, RenderObjectRegistry};

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
            let origin = obj.computed_bounds()
                .map(|b| crate::core::Point::new(
                    absolute_position.x + b.width() * 0.5,
                    absolute_position.y + b.height() * 0.5,
                ))
                .unwrap_or(crate::core::Point::new(absolute_position.x, absolute_position.y));
            ctx.push_command(RenderCommand::PushTransform { transform: *t, origin });
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
            ctx.push_command(RenderCommand::PushClip { bounds: absolute_clip });
        }

        // If this object has a scroll offset, push it before painting children.
        let scroll = obj.scroll_offset();
        if let Some(offset) = &scroll {
            ctx.push_command(RenderCommand::PushOffset { offset: *offset });
        }

        // If this object has an opacity, push it before painting children.
        let opacity = obj.opacity();
        if let Some(opacity_value) = &opacity {
            ctx.push_command(RenderCommand::PushOpacity { opacity: *opacity_value });
        }

        // Paint children
        for child_id in obj.children() {
            Self::paint_recursive(render_objects, *child_id, ctx, absolute_position);
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
