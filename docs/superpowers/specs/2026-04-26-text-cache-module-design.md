# Text Cache Module Design

**Date:** 2026-04-26

## Context

The text cache implementation is currently embedded in `window.rs` with `TextCacheKey`, `CachedTextBuffer`, and cache logic scattered throughout the `WindowState::render()` method. This creates unnecessary coupling between window management and text caching concerns.

## Goal

Extract text cache logic into a dedicated module with full encapsulation, making the codebase more maintainable and the cache logic testable in isolation.

## Design

### Module Structure

Create new file `vexo/src/text_cache.rs`:

```
vexo/src/
├── text_cache.rs          # NEW: TextCache and internal types
├── window.rs              # MODIFIED: Delegates to TextCache
├── lib.rs                 # MODIFIED: Export text_cache module
└── renderer.rs            # UNCHANGED: TextRequest remains here
```

### Public API

```rust
pub struct TextCache { /* private fields */ }

impl TextCache {
    /// Create empty cache
    pub fn new() -> Self;

    /// Get cached buffer or create and cache a new one
    pub fn get_or_create(
        &mut self,
        font_system: &mut glyphon::FontSystem,
        request: &TextRequest,
    ) -> glyphon::Buffer;

    /// Evict entries not used in recent frames
    /// Called periodically by WindowState
    pub fn evict_stale(&mut self);
}
```

### Internal Types (Private)

```rust
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct TextCacheKey {
    content: String,
    font_size_bits: u32,
    color_bits: [u32; 4],
}

impl TextCacheKey {
    fn from_request(request: &TextRequest) -> Self;
}

struct CachedTextBuffer {
    buffer: glyphon::Buffer,
    generation: u64,
}
```

### Cache Behavior

- **Key derivation:** Content string + font size bits + color bits (exact f32 matching)
- **Hit:** Update generation, return cloned buffer
- **Miss:** Create new buffer, shape text, cache it, return clone
- **Eviction:** Every 100 frames, remove entries unused for 100+ frames

### WindowState Changes

Before:
```rust
pub struct WindowState {
    text_cache: HashMap<TextCacheKey, CachedTextBuffer>,
    cache_generation: u64,
    // ...
}
```

After:
```rust
pub struct WindowState {
    text_cache: TextCache,
    // ...
}
```

The render loop simplifies to:
```rust
for req in self.batcher.text_requests.drain(..) {
    let buffer = self.text_cache.get_or_create(
        &mut self.widget_context.font_system,
        &req,
    );
    processed_texts.push((buffer, req));
}

self.text_cache.evict_stale();
```

## Files to Modify

| File | Change |
|------|--------|
| `vexo/src/text_cache.rs` | NEW: Create module with TextCache, TextCacheKey, CachedTextBuffer |
| `vexo/src/window.rs` | Remove inline cache types, use TextCache |
| `vexo/src/lib.rs` | Add `mod text_cache;` and public export |

## Verification

1. `cargo build` compiles without errors
2. `cargo test` passes all existing tests
3. `cargo run -p desktop_demo` renders text correctly
