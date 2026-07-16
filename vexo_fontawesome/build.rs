//! Build script for `vexo_fontawesome`.
//!
//! Parses Font Awesome's official `assets/icons.json` and codegens an `Icons`
//! enum where each variant carries its unicode codepoint. Only the **Free
//! Solid** set is emitted, matching the `fa-solid-900.otf` font registered by
//! [`register_fonts`][crate::register_fonts].
//!
//! # `icons.json` schema (FA 6)
//!
//! The top-level object maps kebab-case icon names to metadata:
//!
//! ```jsonc
//! {
//!   "save": {
//!     "changes": "5.0.0",
//!     "ligatures": [],
//!     "search": { "terms": [ "floppy", "store" ] },
//!     "styles": ["solid", "regular"],   // which styles exist for this icon
//!     "sponsored": { ... },
//!     "free": ["solid"],                 // which styles are in the Free tier
//!     "unicode": "f0c7",
//!     "label": "Save",
//!     "voted": true,
//!     "svg": { "solid": { ... }, "regular": { ... } }
//!   },
//!   "github": {
//!     "styles": ["brands"],
//!     "free": ["brands"],
//!     "unicode": "f09b",
//!     ...
//!   }
//! }
//! ```
//!
//! We emit a variant for every icon whose `free` array contains `"solid"`.

use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct IconEntry {
    unicode: String,
    #[serde(default)]
    free: Vec<String>,
}

/// Reserved Rust keywords / reserved-for-future-use identifiers that cannot
/// be used as raw enum variant names without `r#`. We append a trailing `_`
/// to stay stable across Rust editions and avoid `r#` noise at call sites.
///
/// Source: <https://doc.rust-lang.org/reference/keywords.html>
const RESERVED: &[&str] = &[
    "as", "break", "const", "continue", "crate", "else", "enum", "extern", "false", "fn", "for",
    "if", "impl", "in", "let", "loop", "match", "mod", "move", "mut", "pub", "ref", "return",
    "self", "Self", "static", "struct", "super", "trait", "true", "type", "unsafe", "use", "where",
    "while", "async", "await", "dyn", "abstract", "become", "box", "do", "final", "macro",
    "override", "priv", "typeof", "unsized", "virtual", "yield", "try",
];

fn is_reserved(name: &str) -> bool {
    RESERVED.iter().any(|r| *r == name)
}

/// Convert a kebab-case FA icon name (e.g. `"thumbs-up"`) to a valid PascalCase
/// Rust identifier (e.g. `ThumbsUp`). Handles leading digits and reserved
/// keywords.
fn pascal_case(kebab: &str) -> String {
    let mut out = String::new();
    let mut capitalize_next = true;
    for ch in kebab.chars() {
        if ch == '-' || ch == '_' || ch == ' ' {
            capitalize_next = true;
            continue;
        }
        if capitalize_next {
            out.extend(ch.to_uppercase());
            capitalize_next = false;
        } else {
            out.push(ch);
        }
    }
    // Identifiers cannot start with a digit; prefix with `_`.
    if out.starts_with(|c: char| c.is_ascii_digit()) {
        out.insert(0, '_');
    }
    if is_reserved(&out) {
        out.push('_');
    }
    out
}

/// Escape a unicode codepoint string ("f0c7") into a Rust `&'static str`
/// holding the single corresponding `char`. We emit a `char` escape so the
/// generated source is human-readable and ASCII-safe.
fn codepoint_to_rust_str(cp: &str) -> String {
    // FA codepoints are lowercase hex of arbitrary length; parse to u32.
    let n = u32::from_str_radix(cp.trim_start_matches("0x"), 16)
        .unwrap_or_else(|e| panic!("invalid codepoint '{cp}': {e}"));
    // Validate that the codepoint is a real char (catches corrupt metadata).
    char::from_u32(n).unwrap_or_else(|| panic!("codepoint U+{n:04X} is not a valid char"));
    // Use \u{{...}} so the emitted string is a single-glyph literal.
    format!("\"\\u{{{:x}}}\"", n)
}

fn main() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let icons_json_path = manifest_dir.join("assets/icons.json");
    let otf_path = manifest_dir.join("assets/fa-solid-900.otf");

    // Both assets must exist. `include_bytes!` in lib.rs already enforces the
    // OTF at compile time; we additionally require icons.json for codegen.
    if !icons_json_path.exists() {
        panic!(
            "vexo_fontawesome: assets/icons.json not found at {}.\n\
             Download it from https://github.com/FortAwesome/Font-Awesome (metadata/icons.json) \
             and place it in vexo_fontawesome/assets/. See README.md.",
            icons_json_path.display()
        );
    }
    if !otf_path.exists() {
        panic!(
            "vexo_fontawesome: assets/fa-solid-900.otf not found at {}.\n\
             Download the FontAwesome Free for Web zip from https://fontawesome.com/download \
             and copy webfonts/fa-solid-900.otf into vexo_fontawesome/assets/. See README.md.",
            otf_path.display()
        );
    }

    // Re-run if either asset changes.
    println!("cargo:rerun-if-changed={}", icons_json_path.display());
    println!("cargo:rerun-if-changed={}", otf_path.display());

    let json = fs::read_to_string(&icons_json_path)
        .unwrap_or_else(|e| panic!("failed to read {}: {e}", icons_json_path.display()));

    // BTreeMap for deterministic, alphabetically-sorted variant order.
    let entries: BTreeMap<String, IconEntry> =
        serde_json::from_str(&json).unwrap_or_else(|e| panic!("invalid icons.json: {e}"));

    let mut variants: Vec<(String, String, String)> = Vec::new(); // (variant, original_name, codepoint_str)
    for (name, entry) in &entries {
        if !entry.free.iter().any(|s| s == "solid") {
            continue;
        }
        let variant = pascal_case(name);
        let cp = codepoint_to_rust_str(&entry.unicode);
        variants.push((variant, name.clone(), cp));
    }

    if variants.is_empty() {
        panic!(
            "vexo_fontawesome: no Free Solid icons found in icons.json. \
             Is this the correct FA6 metadata file?"
        );
    }

    let mut out = String::new();
    out.push_str("// AUTO-GENERATED by build.rs — DO NOT EDIT BY HAND.\n");
    out.push_str("// Source: Font Awesome Free 6, `icons.json`, filtered to `free`/`solid`.\n\n");

    // Build the enum. We attach the original kebab-case name as a doc comment
    // for discoverability ("which icon is `ThumbsUp`? -> thumbs-up").
    out.push_str("/// All Font Awesome 6 Free **Solid** icons, as a typed enum.\n");
    out.push_str("///\n");
    out.push_str("/// Each variant maps to a unicode codepoint in `fa-solid-900.otf`.\n");
    out.push_str("/// Use with [`crate::Icon::new`] to render an icon:\n");
    out.push_str("///\n");
    out.push_str("/// ```ignore\n");
    out.push_str("/// use vexo_fontawesome::{Icon, Icons};\n");
    out.push_str("/// Icon::new(Icons::House).with_size(24.0).boxed()\n");
    out.push_str("/// ```\n");
    out.push_str("#[allow(non_camel_case_types)]\n");
    out.push_str("#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]\n");
    out.push_str("pub enum Icons {\n");
    for (variant, original, _cp) in &variants {
        out.push_str(&format!("    /// Font Awesome icon `{}`.\n", original));
        out.push_str(&format!("    {},\n", variant));
    }
    out.push_str("}\n\n");

    // `codepoint()` — returns the single-glyph string for this icon.
    out.push_str("impl Icons {\n");
    out.push_str("    /// The font family this icon belongs to.\n");
    out.push_str("    pub const FONT_FAMILY: &'static str = \"Font Awesome 7 Free\";\n\n");
    out.push_str("    /// The font family name to pass to `Text::with_font_family`.\n");
    out.push_str("    pub fn family(&self) -> &'static str {\n");
    out.push_str("        Self::FONT_FAMILY\n");
    out.push_str("    }\n\n");
    out.push_str("    /// The unicode codepoint for this icon, as a single-glyph string\n");
    out.push_str("    /// (e.g. `\"\\u{f015}\"` for `House`).\n");
    out.push_str("    pub fn codepoint(&self) -> &'static str {\n");
    out.push_str("        match self {\n");
    for (variant, _original, cp) in &variants {
        out.push_str(&format!("            Icons::{} => {},\n", variant, cp));
    }
    out.push_str("        }\n");
    out.push_str("    }\n");
    out.push_str("}\n");

    let out_dir = PathBuf::from(std::env::var("OUT_DIR").expect("OUT_DIR not set"));
    let dest = out_dir.join("icons.rs");
    fs::write(&dest, out).unwrap_or_else(|e| panic!("failed to write {}: {e}", dest.display()));

    println!(
        "cargo:warning=vexo_fontawesome: codegen emitted {} Solid icons",
        variants.len()
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pascal_case_basic() {
        assert_eq!(pascal_case("save"), "Save");
        assert_eq!(pascal_case("thumbs-up"), "ThumbsUp");
        assert_eq!(pascal_case("arrow-right-long"), "ArrowRightLong");
    }

    #[test]
    fn pascal_case_leading_digit() {
        assert_eq!(pascal_case("0"), "_0");
        assert_eq!(pascal_case("4k"), "_4k");
    }

    #[test]
    fn pascal_case_reserved_keyword() {
        // Lowercase keywords become PascalCase, which are NOT keywords
        // (e.g. `loop` → `Loop`, `box` → `Box`) — these are valid variant
        // names accessed as `Icons::Loop`, `Icons::Box`.
        assert_eq!(pascal_case("loop"), "Loop");
        assert_eq!(pascal_case("box"), "Box");
        assert_eq!(pascal_case("type"), "Type");
        // `Self` is the one capitalized reserved word — it must be escaped.
        assert_eq!(pascal_case("self"), "Self_");
    }

    #[test]
    fn codepoint_escape_is_valid() {
        let s = codepoint_to_rust_str("f0c7");
        // Should be a quoted Rust string literal with a \u{...} escape.
        assert!(s.starts_with('"') && s.ends_with('"'));
    }
}
