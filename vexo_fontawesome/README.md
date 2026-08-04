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

The FontAwesome 6 Free Solid font (`fa-solid-900.otf`) and metadata
(`icons.json`) are bundled with this crate, so `cargo add vexo_fontawesome`
works with no manual asset download. Both are third-party assets from
[Font Awesome 6 Free](https://fontawesome.com), licensed under
[SIL OFL 1.1](https://scripts.sil.org/OFL) (font) and
[CC BY 4.0](https://creativecommons.org/licenses/by/4.0/) (icons) — see
<https://fontawesome.com/license/free> for full attribution.

### Register the font with your app

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

### Use icons

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
