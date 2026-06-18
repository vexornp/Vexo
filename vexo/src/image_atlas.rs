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
}

impl ShelfAllocator {
    pub fn new(atlas_width: u32, atlas_height: u32) -> Self {
        Self {
            atlas_width,
            atlas_height,
            shelves: Vec::new(),
            next_key: 0,
            images: HashMap::new(),
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
    pub fn remove(&mut self, key: ImageKey) {
        self.images.remove(&key);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shelf_allocator_single_image() {
        let mut alloc = ShelfAllocator::new(1024, 1024);
        let (key, region) = alloc.allocate(100, 50);
        assert_eq!(region, AtlasRegion { x: 0, y: 0, width: 100, height: 50 });
        assert_eq!(alloc.get_region(key), Some(&region));
    }

    #[test]
    fn shelf_allocator_two_images_same_shelf() {
        let mut alloc = ShelfAllocator::new(1024, 1024);
        let (_, r1) = alloc.allocate(100, 50);
        let (_, r2) = alloc.allocate(200, 50);
        assert_eq!(r1, AtlasRegion { x: 0, y: 0, width: 100, height: 50 });
        assert_eq!(r2, AtlasRegion { x: 100, y: 0, width: 200, height: 50 });
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
}
