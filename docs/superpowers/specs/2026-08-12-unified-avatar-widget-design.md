# Unified Avatar Widget — Design

## Problem

`shared_app` has four avatar call sites with inconsistent patterns:

| Site | Diameter | Source type | Ring? | Badge? | Decode cache? |
|------|----------|-------------|-------|--------|---------------|
| `conversation_list.rs` | 40px | `AvatarSource` (Bytes\|Url) | yes (manual) | yes (manual Stack) | no |
| `chat_screen.rs` | 32px | `AvatarSource` (Bytes\|Url) | no | no | yes (manual `them/me_avatar_image`) |
| `contacts_screen.rs` | 40px | `Rc<[u8]>` only | no | no | no |
| `profile_screen.rs` | 56px | `Rc<[u8]>` only | no | no | no |

Three pain points:

1. The Bytes/Url branching is duplicated in `conversation_list` and `chat_screen`.
2. The data model is split — `Conversation` uses `AvatarSource` but `Contact`/`Profile` use bare `Rc<[u8]>`.
3. The ring + unread badge are hand-composed in a `Stack` by `conversation_list`, and the PNG decode cache is manual and local to `chat_screen`. Three other sites re-decode the PNG on every `render()`.

## Goal

One stateful `Avatar` Component replaces the three free functions in `widgets/avatar.rs` (`avatar`, `network_avatar`, `avatar_border_ring`) and the hand-wired Stack/ring/badge/caching scattered across the four call sites.

## Decisions

- **Responsibility boundary — full slot.** The Avatar widget owns the clipped circle AND the optional ring AND the optional unread badge. All four call sites pass data, not widgets.
- **Data model — unify to `AvatarSource`.** `Contact.avatar_bytes` and `Profile.avatar_bytes` become `avatar: AvatarSource`. The widget takes `AvatarSource` everywhere — no `Into` normalization, no per-call-site branching.
- **Decode caching — widget owns it.** The Avatar widget is a stateful `Component` holding `Option<ImageData>` in its State, lazily decoding on first render and reusing thereafter. `chat_screen` drops its manual `them_avatar_image`/`me_avatar_image` cache. All four sites get caching for free.
- **API shape — builder + typed badge.** `Avatar::new(source, diameter).with_ring(bool).with_unread_badge(count)`. The widget owns badge rendering; the existing `unread_badge` helper moves into the avatar module.

## Architecture

### Data model unification (`shared_app/src/data.rs`)

- `Contact.avatar_bytes: Rc<[u8]>` → `Contact.avatar: AvatarSource`
- `Profile.avatar_bytes: Rc<[u8]>` → `Profile.avatar: AvatarSource`
- `AvatarSource` (already `Bytes(Rc<[u8]>) | Url(Url)`) becomes the single avatar input type across the whole app.
- `seed()` updated: `Contact`/`Profile` literals wrap their bytes in `AvatarSource::Bytes(...)`. No new behavior — just a wrapper. (The "Me" avatar stays `make_avatar_png(130, 100, 200)`.)
- `Conversation` is unchanged (already `AvatarSource`).

### Module layout

- `shared_app/src/widgets/avatar.rs` becomes the home of the `Avatar` Component (struct + `Component` impl + State) and absorbs the `unread_badge` helper (moved from `conversation_list.rs`) as a private fn used by the widget's render.
- The three old free fns (`avatar`, `network_avatar`, `avatar_border_ring`) are removed — all callers migrate to `Avatar`. No deprecated shims (this is an internal crate; shims would just delay the deletion).

### Why this shape

Unifying the data type is what makes "one widget, reused everywhere" honest — the widget takes exactly one input type. Moving `unread_badge` into the avatar module co-locates the badge renderer with the widget that paints it.

## Widget Internals

### The `Avatar` Component (`widgets/avatar.rs`)

```rust
#[derive(Clone)]
pub(crate) struct Avatar {
    source: AvatarSource,
    diameter: f32,
    ring: bool,
    unread_badge: Option<u32>,  // None = no badge; Some(0) is treated as no badge
}
```

### Builder API

- `Avatar::new(source: AvatarSource, diameter: f32) -> Self` — defaults `ring: false`, `unread_badge: None`.
- `.with_ring(bool)` — enables the 1px outline ring (color from `Theme::of(ctx).outline` at render time, so it re-themes correctly).
- `.with_unread_badge(count: u32)` — sets `unread_badge: Some(count)`. Render treats `0`/`None` identically (no badge drawn).

This mirrors `Text::new(...).with_font_size(...)` conventions already in the codebase.

### State (decode cache)

```rust
struct AvatarState {
    image: Option<ImageData>,  // lazily decoded, reused across renders
}
```

- `Component::State` via `#[derive(ComponentState)]` or a manual impl, following the same pattern as `ConversationRowState`.
- On `render()`: if `source` is `Bytes(b)` and `image` is `None`, decode once via `ImageData::from_bytes(b)` and store. Subsequent renders reuse. If `source` is `Url(_)`, no decode needed — `NetworkImage` + `ImageCache` handle it.
- **Cache invalidation:** `should_rebuild()` compares the new `Avatar`'s `source`/`diameter`/`ring`/`unread_badge` against the previous. On `Bytes` source change, the cache is invalidated (set `image = None`) so a new conversation's avatar doesn't show the stale decoded image. On `Url` change, `NetworkImage`'s own key-based reconciliation handles it — no state work.

  *Note on `should_rebuild` usage:* `Avatar` is a leaf display widget, not in the keyboard-animation hot path (the three level-3 users are `ChatScreen`, `TabBarView`, `NavigationStackView`). So the default `should_rebuild() == true` is correct here — no manual override, no `Memo` needed. The cache invalidation happens in `mount`/`update`, not via `should_rebuild`.

### Render() output — a `Stack` of `diameter × diameter`

1. **Base layer:** `ClipRRect(circle)` wrapping either `Image(cached)` (Bytes) or `NetworkImage(url).with_key(url_str)` (Url), both sized via `WithLayout` to pin `diameter × diameter` (layout-stable across load states, same as today).
2. **If `ring`:** the `Positioned` border-ring overlay (the current `avatar_border_ring` logic, inlined as a private fn) pushed above the image.
3. **If `unread_badge > 0`:** `unread_badge(count, theme)` wrapped in `Positioned().top(-4.0).right(-4.0)`, pushed last (top of stack).

Stack layout: `Layout::stack().width(diameter).height(diameter).flex_shrink(0.0)` — identical to the current `conversation_list` Stack, so layout is byte-for-byte equivalent.

### `chat_screen` migration detail

`chat_screen` currently caches *both* `them_avatar_image` and `me_avatar_image` in its own state because it renders two distinct avatars (me + them) per frame. After migration, each `Avatar` widget instance owns its own cache, so `chat_screen`'s `them_avatar_image`/`me_avatar_image` fields and the `them_avatar()`/`me_avatar()` methods are deleted. The per-row `avatar_widget` construction collapses from the 10-line `if is_me { ... } match { ... }` block to `Avatar::new(src, 32.0)` where `src` is `AvatarSource::Bytes(me_bytes)` or `self.avatar.clone()`.

## Call-Site Migration

Each of the four call sites becomes a one-liner:

| Site | Before | After |
|------|--------|-------|
| `conversation_list.rs` (40px) | 15-line block: match Bytes/Url → `avatar`/`network_avatar`, manual `avatar_border_ring`, manual `unread_badge`, manual `Stack` w/ badge `Positioned` | `Avatar::new(conv.avatar.clone(), 40.0).with_ring(true).with_unread_badge(conv.unread_count)` |
| `chat_screen.rs` (32px) | 10-line `if is_me {…} match {Bytes/Url}` + `them/me_avatar_image` cache fields + `them_avatar()`/`me_avatar()` methods | `Avatar::new(src, 32.0)` where `src` is `AvatarSource::Bytes(me_bytes.clone())` or `self.avatar.clone()`; delete the cache fields/methods |
| `contacts_screen.rs` (40px) | `avatar(ImageData::from_bytes(&c.avatar_bytes).expect(...), 40.0)` | `Avatar::new(c.avatar.clone(), 40.0)` |
| `profile_screen.rs` (56px) | `avatar(ImageData::from_bytes(&profile.avatar_bytes).expect(...), 56.0)` | `Avatar::new(profile.avatar.clone(), 56.0)` |

### `data.rs` seed updates

- Every `Contact { avatar_bytes: foo_bytes.clone() }` → `avatar: AvatarSource::Bytes(foo_bytes.clone())`.
- `Profile { avatar_bytes: me_bytes }` → `avatar: AvatarSource::Bytes(me_bytes)`.
- `make_avatar_png` unchanged. `Conversation` literals unchanged (already `AvatarSource`).

### Removed code

- `widgets/avatar.rs`: the three free fns `avatar`, `network_avatar`, `avatar_border_ring`.
- `conversation_list.rs`: the local `unread_badge` fn (absorbed into `avatar.rs` as private).
- `chat_screen.rs`: `them_avatar_image`/`me_avatar_image` fields, `them_avatar()`/`me_avatar()` methods.

## Behavioral Preservation

Each of these is a no-regression guarantee:

- **Ring/badge placement:** Ring and badge only appear where they appeared before (`conversation_list` only). `chat_screen`/`contacts`/`profile` use `.with_ring(false)` implicitly — same as today's no-ring.
- **Layout:** Stack sizing (`diameter × diameter`, `flex_shrink(0.0)`) and badge offset (`-4.0 / -4.0`) are carried over verbatim — pixel-equivalent.
- **Theming:** `Avatar` reads `Theme::of(ctx)` in `render()` for ring color and badge colors (`outline`, `error`, `on_error`), same as `conversation_list` does today. Re-themes on `Theme` swap (existing `InheritedWidget` invalidation path).
- **Decode caching:** Behavior improves for `conversation_list`/`contacts`/`profile` (no per-frame PNG decode), unchanged for `chat_screen` (still cached, just owned by the widget instead of the screen).

## Testing

### Existing integration tests stay green

`chat_screen` tests (`seed_avatar`, `seed_me_avatar`, click-coordinate tests at avatar offsets) assert pixel offsets that depend on the avatar's `diameter × diameter` slot. Those offsets are preserved, so no test edits beyond the `seed_avatar`/`seed_me_avatar` helpers adapting to the `AvatarSource` field rename. Verified by running `cargo test`.

### New unit tests in `widgets/avatar.rs`

- `avatar_renders_bytes_without_panic` — `Avatar::new(Bytes(...), 40.0)` through `ThreeTreePipeline::update`; assert element count > 0.
- `avatar_renders_url_without_panic` — same with a `Url` source (no network in test; `NetworkImage` loading state is blank-by-design).
- `avatar_with_badge_and_ring` — assert the Stack has the expected extra layers (element count strictly greater than the bare-avatar case).
- `avatar_caches_decode` — build the widget, render twice through the pipeline, and assert the decode happens exactly once. Mechanism: expose a `#[cfg(test)]` decode counter on `AvatarState` (incremented in the decode branch) and assert it equals 1 after two `pipeline.update()` calls. Locks the caching contract.

### `data.rs` test update

`test_avatar_bytes_decode` stays; add `test_contact_and_profile_use_avatar_source` asserting both fields are `AvatarSource::Bytes` after `seed()`.

## Out of Scope (YAGNI)

- No online-presence dot.
- No load spinner/error placeholder for `NetworkImage`.
- No tap handler.
- No size variants (callers pass the diameter).

These can be added later via new builder methods without breaking the API.
