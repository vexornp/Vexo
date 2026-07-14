use std::collections::HashMap;

/// Unique identifier for an image registered in the atlas.
pub type ImageKey = u64;

/// A region within the atlas texture where an image is stored.
#[derive(Clone, Debug, PartialEq)]
pub struct AtlasRegion {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

/// A horizontal strip in the shelf allocator.
pub struct Shelf {
    pub y: u32,
    pub height: u32,
    pub x_cursor: u32,
    pub remaining_width: u32,
}

/// Pure-data shelf allocator for atlas packing (no GPU resources).
pub struct ShelfAllocator {
    atlas_width: u32,
    atlas_height: u32,
    shelves: Vec<Shelf>,
    next_key: ImageKey,
    images: HashMap<ImageKey, AtlasRegion>,
    /// Regions returned by `remove()` that can be reused by future `allocate()` calls.
    /// Without this free list, removed images would permanently leak their shelf
    /// space (the shelf's `x_cursor`/`remaining_width` are monotonic).
    free_regions: Vec<AtlasRegion>,
}

impl ShelfAllocator {
    pub fn new(atlas_width: u32, atlas_height: u32) -> Self {
        Self {
            atlas_width,
            atlas_height,
            shelves: Vec::new(),
            next_key: 0,
            images: HashMap::new(),
            free_regions: Vec::new(),
        }
    }

    pub fn atlas_width(&self) -> u32 {
        self.atlas_width
    }

    pub fn atlas_height(&self) -> u32 {
        self.atlas_height
    }

    /// Allocate a region for an image of the given size.
    /// Returns the ImageKey and AtlasRegion, or panics if atlas is full.
    pub fn allocate(&mut self, width: u32, height: u32) -> (ImageKey, AtlasRegion) {
        // First, try to reuse a freed region (first-fit). This is what keeps the
        // atlas from filling up under repeated push/pop cycles: an image removed
        // by `remove()` releases its slot here, so the next registration of a
        // same-or-smaller image can take it instead of carving new shelf space.
        if let Some(idx) = self
            .free_regions
            .iter()
            .position(|r| r.width >= width && r.height >= height)
        {
            let slot = self.free_regions.swap_remove(idx);
            // Place the image at the top-left of the freed slot. If the slot is
            // larger than requested, split the leftover horizontally and return
            // it to the free list to avoid internal fragmentation.
            let placed = AtlasRegion {
                x: slot.x,
                y: slot.y,
                width,
                height,
            };
            if slot.width > width {
                self.free_regions.push(AtlasRegion {
                    x: slot.x + width,
                    y: slot.y,
                    width: slot.width - width,
                    height: slot.height,
                });
            }
            if slot.height > height {
                self.free_regions.push(AtlasRegion {
                    x: slot.x,
                    y: slot.y + height,
                    width,
                    height: slot.height - height,
                });
            }
            let key = self.next_key;
            self.next_key += 1;
            self.images.insert(key, placed.clone());
            return (key, placed);
        }

        // Find a shelf with enough remaining width whose height >= image height
        for shelf in &mut self.shelves {
            if shelf.remaining_width >= width && shelf.height >= height {
                let region = AtlasRegion {
                    x: shelf.x_cursor,
                    y: shelf.y,
                    width,
                    height,
                };
                shelf.x_cursor += width;
                shelf.remaining_width -= width;
                let key = self.next_key;
                self.next_key += 1;
                self.images.insert(key, region.clone());
                return (key, region);
            }
        }

        // Create a new shelf at current y offset
        let y_offset = self.shelves.last().map_or(0, |s| s.y + s.height);
        if y_offset + height > self.atlas_height {
            panic!(
                "Image atlas is full: cannot fit {}x{} image. Atlas size: {}x{}",
                width, height, self.atlas_width, self.atlas_height
            );
        }

        let shelf = Shelf {
            y: y_offset,
            height,
            x_cursor: 0,
            remaining_width: self.atlas_width,
        };
        self.shelves.push(shelf);

        let shelf = self.shelves.last_mut().unwrap();
        let region = AtlasRegion {
            x: shelf.x_cursor,
            y: shelf.y,
            width,
            height,
        };
        shelf.x_cursor += width;
        shelf.remaining_width -= width;
        let key = self.next_key;
        self.next_key += 1;
        self.images.insert(key, region.clone());
        (key, region)
    }

    /// Look up a previously allocated region.
    pub fn get_region(&self, key: ImageKey) -> Option<&AtlasRegion> {
        self.images.get(&key)
    }

    /// Remove a region from the atlas.
    ///
    /// The region is returned to a free list so a subsequent `allocate()` of
    /// the same or smaller size can reuse it. This is critical for the image
    /// atlas lifecycle: without it, every push/pop cycle on iOS leaks a slot
    /// and the 2048x2048 atlas fills up after a few dozen navigations.
    pub fn remove(&mut self, key: ImageKey) {
        if let Some(region) = self.images.remove(&key) {
            self.free_regions.push(region);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shelf_allocator_single_image() {
        let mut alloc = ShelfAllocator::new(1024, 1024);
        let (key, region) = alloc.allocate(100, 50);
        assert_eq!(
            region,
            AtlasRegion {
                x: 0,
                y: 0,
                width: 100,
                height: 50
            }
        );
        assert_eq!(alloc.get_region(key), Some(&region));
    }

    #[test]
    fn shelf_allocator_two_images_same_shelf() {
        let mut alloc = ShelfAllocator::new(1024, 1024);
        let (_, r1) = alloc.allocate(100, 50);
        let (_, r2) = alloc.allocate(200, 50);
        assert_eq!(
            r1,
            AtlasRegion {
                x: 0,
                y: 0,
                width: 100,
                height: 50
            }
        );
        assert_eq!(
            r2,
            AtlasRegion {
                x: 100,
                y: 0,
                width: 200,
                height: 50
            }
        );
    }

    #[test]
    fn shelf_allocator_new_shelf_for_taller_image() {
        let mut alloc = ShelfAllocator::new(1024, 1024);
        let (_, r1) = alloc.allocate(100, 50);
        let (_, r2) = alloc.allocate(100, 80);
        assert_eq!(r1.y, 0);
        assert_eq!(r2.y, 50);
    }

    #[test]
    fn shelf_allocator_remove_image() {
        let mut alloc = ShelfAllocator::new(1024, 1024);
        let (key, _) = alloc.allocate(100, 50);
        alloc.remove(key);
        assert!(alloc.get_region(key).is_none());
    }

    #[test]
    #[should_panic(expected = "Image atlas is full")]
    fn shelf_allocator_full_atlas_panics() {
        let mut alloc = ShelfAllocator::new(100, 100);
        alloc.allocate(50, 60);
        alloc.allocate(50, 60);
        // Third image would exceed atlas height
        alloc.allocate(50, 60);
    }

    /// Regression test for the iOS push/pop atlas leak.
    ///
    /// Before the free list, `remove()` only deleted the HashMap entry, so the
    /// shelf space was never reclaimed. Allocating the same size image in a
    /// tight loop would burn through the atlas and panic. With the free list,
    /// a removed region is handed back to the next `allocate()` of compatible
    /// size, so the loop must run indefinitely without growing the atlas.
    #[test]
    fn shelf_allocator_reuses_freed_region() {
        let mut alloc = ShelfAllocator::new(128, 128);
        // Fill well past what the atlas could hold without reuse: 1000 cycles
        // of allocate -> remove for the same 64x64 image. Without the free
        // list, the first iteration would consume shelf [0..64] and each
        // subsequent allocate would carve a new shelf, panicking by cycle ~3.
        for _ in 0..1000 {
            let (key, region) = alloc.allocate(64, 64);
            assert_eq!(region.width, 64);
            assert_eq!(region.height, 64);
            alloc.remove(key);
        }
        // After all the churn, the allocator should still be able to place a
        // fresh image without exceeding the atlas.
        let (_, r) = alloc.allocate(64, 64);
        assert_eq!(
            r,
            AtlasRegion {
                x: 0,
                y: 0,
                width: 64,
                height: 64
            }
        );
    }

    /// A smaller image should fit into a larger freed slot, with the leftover
    /// retained for future allocations.
    #[test]
    fn shelf_allocator_reuses_smaller_into_larger_slot() {
        let mut alloc = ShelfAllocator::new(256, 256);
        let (k_big, _) = alloc.allocate(100, 100);
        alloc.remove(k_big);
        // Smaller image must reuse the freed slot instead of carving a new shelf.
        let (k_small, r_small) = alloc.allocate(40, 40);
        assert_eq!(r_small.x, 0);
        assert_eq!(r_small.y, 0);
        // The leftover (60x100 and 60x100) should now host another allocation.
        let (_, r2) = alloc.allocate(40, 40);
        // Either to the right of the small image (split horizontally) or below.
        assert!(
            (r2.x == 40 && r2.y == 0) || (r2.x == 0 && r2.y == 40),
            "expected leftover reuse, got {:?}",
            r2
        );
        alloc.remove(k_small);
    }
}
