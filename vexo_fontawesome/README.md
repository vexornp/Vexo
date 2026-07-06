# vexo_fontawesome

[FontAwesome 6 Free](https://fontawesome.com) (Solid style) icon widgets for the
[Vexo](../vexo) UI framework.

Icons are addressed by a strongly-typed enum (`Icons`) instead of raw unicode
codepoints — typos are caught at compile time and IDEs can autocomplete icon
names.

```rust
use vexo_fontawesome::{Icon, Icons};

Icon::new(Icons::House)
    .with_size(24.0)
    .with_color(vexo::Color::BLACK)
    .boxed()
```

## Setup

This crate embeds the FontAwesome font at compile time via `include_bytes!`,
so two asset files **must** be present under `vexo_fontawesome/assets/` before
the crate will build. Both are excluded from git (see `.gitignore`) because
they are third-party downloads.

### 1. Download `fa-solid-900.otf`

> **Important:** You need the **OTF** format specifically. The "Free for Web"
> zip only contains `.woff2` / `.woff` / `.ttf`, which **will not work** —
> vexo's font stack (`fontdb` / `ttf-parser`) only parses `.ttf` / `.otf` /
> `.ttc` / `.otc`.

1. Go to <https://fontawesome.com/download>.
2. Download the **Free for Desktop** zip (contains `.otf` files).
3. Copy `otfs/Font Awesome 6 Free-Solid-900.otf` to
   `vexo_fontawesome/assets/fa-solid-900.otf`.

   (The desktop zip names the file `Font Awesome 6 Free-Solid-900.otf` with
   spaces; rename it to `fa-solid-900.otf` so the `include_bytes!` path in
   `src/lib.rs` matches.)

### 2. Download `icons.json`

1. Go to <https://github.com/FortAwesome/Font-Awesome>.
2. Copy `metadata/icons.json` to `vexo_fontawesome/assets/icons.json`.

(Or, from a checkout of the FA repo: `cp metadata/icons.json path/to/vexo_fontawesome/assets/`.)

### 3. Register the font with your app

Implement `Application::register_fonts` and forward to this crate:

```rust
impl vexo::Application for MyApp {
    type State = MyState;
    fn new() -> Self::State { /* ... */ }
    fn view(state: &mut Self::State) -> Box<dyn vexo::Widget> { /* ... */ }

    fn register_fonts(fs: &mut glyphon::FontSystem) {
        vexo_fontawesome::register_fonts(fs);
    }
}
```

### 4. Use icons

```rust
use vexo_fontawesome::{Icon, Icons};
use vexo::{Color, Widget};

fn toolbar() -> Box<dyn Widget> {
    Icon::new(Icons::House).with_size(20.0).with_color(Color::BLACK).boxed()
}
```

## What's included

- The `Icons` enum is **code-generated** at build time from `icons.json`,
  filtered to icons whose `free` array contains `"solid"`. This is the full
  FA6 Free Solid set (~2,000 icons).
- Variant names are PascalCase of the FA kebab-case names
  (`"thumbs-up"` → `ThumbsUp`, `"floppy-disk"` → `FloppyDisk`). Reserved
  Rust keywords get a trailing `_` (e.g. a hypothetical `"self"` → `Self_`;
  note `"loop"` → `Loop` and `"box"` → `Box` are fine since capitalized
  forms are not keywords).

## Why only Solid?

FontAwesome 6 Free has three styles:

| Style | OTF | Family name | Weight |
|---|---|---|---|
| Solid | `fa-solid-900.otf` | `Font Awesome 6 Free` | 900 |
| Regular | `fa-regular-400.otf` | `Font Awesome 6 Free` | 400 |
| Brands | `fa-brands-400.otf` | `Font Awesome 6 Brands` | 400 |

**Solid and Regular share the same family name** but differ only in weight.
Vexo's `Text` widget exposes `font_family` but not `font_weight`, so loading
both would cause them to collide (whichever is registered last wins for a given
codepoint). We therefore ship Solid only.

Brands has a distinct family name and could be added in the future without
collision; file an issue if you need it.

## License

The Rust code in this crate is licensed under the same terms as the Vexo
framework.

**Font Awesome Free is licensed under its own terms** (icons: CC BY 4.0,
fonts: SIL OFL 1.1). See <https://fontawesome.com/license/free> for details.
You are responsible for compliance with the FontAwesome license when
distributing the embedded OTF.
