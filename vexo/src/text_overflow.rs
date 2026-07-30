//! Text overflow/ellipsis algorithm.
//!
//! Pure, GPU-free function that truncates text to fit within a maximum
//! number of lines, appending `…` when truncation occurs.

use glyphon::{Attrs, Buffer, Family, FontSystem, Metrics, Shaping};

/// Result of truncating text to fit within `max_lines`.
pub struct TruncationResult {
    /// Content after truncation. Ends with `…` iff truncation occurred.
    pub content: String,
    /// Number of visible lines in the truncated content.
    pub line_count: usize,
    /// Height of the truncated content in logical pixels
    /// (`line_count * font_size * line_height`).
    pub height: f32,
}

/// Truncate `content` to fit within `max_lines` lines at `max_width`,
/// appending `…` if truncation occurred.
///
/// - `max_width`: wrap width in logical pixels. `None` means unbounded
///   (only `\n` breaks lines).
/// - `max_lines`: maximum number of visible lines. Values < 1 are treated as 1.
pub fn truncate_with_ellipsis(
    content: &str,
    font_size: f32,
    line_height: f32,
    font_family: Option<&str>,
    max_width: Option<f32>,
    max_lines: u32,
    font_system: &mut FontSystem,
) -> TruncationResult {
    let max_lines = max_lines.max(1) as usize;

    // Shape the full content and count lines.
    let natural_lines = count_lines(
        content,
        font_size,
        line_height,
        font_family,
        max_width,
        font_system,
    );

    // Fast path: content already fits — no truncation.
    if natural_lines <= max_lines {
        let line_count = if content.is_empty() { 0 } else { natural_lines };
        return TruncationResult {
            content: content.to_string(),
            line_count,
            height: line_count as f32 * font_size * line_height,
        };
    }

    // Empty content edge case (defensive — already caught by fast path
    // since count_lines returns 0 for empty, but guard anyway).
    if content.is_empty() {
        return TruncationResult {
            content: String::new(),
            line_count: 0,
            height: font_size * line_height,
        };
    }

    // max_width = None: only \n breaks lines. No width-wrapping, so
    // "…" appended to the last kept line never causes a new line.
    // Just split by \n, keep first max_lines, append ….
    if max_width.is_none() {
        let lines: Vec<&str> = content.split('\n').collect();
        let kept: Vec<&str> = lines.iter().take(max_lines).copied().collect();
        let mut truncated = kept.join("\n");
        truncated.push('…');
        return TruncationResult {
            content: truncated,
            line_count: max_lines,
            height: max_lines as f32 * font_size * line_height,
        };
    }

    // max_width = Some: binary search for the longest character prefix
    // such that content[..cut_byte] + "…" fits in max_lines lines.
    let char_byte_offsets: Vec<usize> = content.char_indices().map(|(i, _)| i).collect();

    // Check if even empty prefix + "…" fits (max_width too narrow for …).
    let ellipsis_only = "…".to_string();
    if count_lines(
        &ellipsis_only,
        font_size,
        line_height,
        font_family,
        max_width,
        font_system,
    ) > max_lines
    {
        // "…" alone wraps past max_lines — return just "…"
        return TruncationResult {
            content: ellipsis_only,
            line_count: 1,
            height: font_size * line_height,
        };
    }

    // Binary search: find the largest k (chars to keep) in 0..=len such that
    // content[..end_of_k_chars] + "…" fits in max_lines lines at max_width.
    // Predicate is monotonic: fewer chars → fewer or equal lines.
    let mut lo: usize = 0;
    let mut hi: usize = char_byte_offsets.len();

    while lo < hi {
        let mid = lo + (hi - lo + 1) / 2; // round up so lo advances
                                          // end-of-k-chars byte offset: char_byte_offsets[k] for k < len,
                                          // content.len() for k == len (keep all chars).
        let cut_byte = char_byte_offsets.get(mid).copied().unwrap_or(content.len());
        let candidate = format!("{}…", &content[..cut_byte]);
        if count_lines(
            &candidate,
            font_size,
            line_height,
            font_family,
            max_width,
            font_system,
        ) <= max_lines
        {
            lo = mid; // mid fits, try keeping more
        } else {
            hi = mid - 1; // mid too long
        }
    }

    let cut_byte = char_byte_offsets.get(lo).copied().unwrap_or(content.len());
    let truncated = format!("{}…", &content[..cut_byte]);

    // Verify line count (defensive — binary search guarantees this).
    let result_lines = count_lines(
        &truncated,
        font_size,
        line_height,
        font_family,
        max_width,
        font_system,
    )
    .max(1);

    TruncationResult {
        content: truncated,
        line_count: result_lines,
        height: result_lines as f32 * font_size * line_height,
    }
}

/// Shape text at `max_width` and count the number of layout lines.
fn count_lines(
    content: &str,
    font_size: f32,
    line_height: f32,
    font_family: Option<&str>,
    max_width: Option<f32>,
    font_system: &mut FontSystem,
) -> usize {
    if content.is_empty() {
        return 0;
    }

    let metrics = Metrics::new(font_size, font_size * line_height);
    let mut buffer = Buffer::new(font_system, metrics);
    buffer.set_size(max_width, None);

    let mut attrs = Attrs::new();
    if let Some(fam) = font_family {
        attrs = attrs.family(Family::Name(fam));
    }
    buffer.set_text(content, &attrs, Shaping::Advanced, None);
    buffer.shape_until_scroll(font_system, true);

    // Each LayoutRun corresponds to one visual (wrapped) line. Multiple
    // wrapped lines within a single paragraph share the same `line_i`, so
    // counting distinct `line_i` values would undercount; counting runs is
    // the correct way to count visual lines.
    let mut count = 0usize;
    for _ in buffer.layout_runs() {
        count += 1;
    }

    count
}

#[cfg(test)]
mod tests {
    use super::*;

    fn font_system() -> FontSystem {
        let font_data = include_bytes!("../font.ttf").to_vec();
        glyphon::FontSystem::new_with_fonts([glyphon::fontdb::Source::Binary(std::sync::Arc::new(
            font_data,
        ))])
    }

    #[test]
    fn test_no_truncation_when_fits() {
        // Short text, max_lines=5 — no truncation needed.
        let mut fs = font_system();
        let result = truncate_with_ellipsis("Hello", 24.0, 1.2, None, None, 5, &mut fs);
        assert_eq!(result.content, "Hello");
        assert!(!result.content.ends_with('…'));
        assert_eq!(result.line_count, 1);
    }

    #[test]
    fn test_truncation_appends_ellipsis() {
        // Long text at narrow width, max_lines=1 — must truncate.
        let mut fs = font_system();
        let result = truncate_with_ellipsis(
            "This is a long text that should be truncated",
            24.0,
            1.2,
            None,
            Some(100.0),
            1,
            &mut fs,
        );
        assert!(
            result.content.ends_with('…'),
            "truncated content must end with …, got: {}",
            result.content
        );
        assert!(
            result.content.len() < "This is a long text that should be truncated".len() + 3,
            "truncated content should be shorter than original + ellipsis"
        );
        assert_eq!(result.line_count, 1);
    }

    #[test]
    fn test_newlines_count_toward_limit() {
        let mut fs = font_system();
        // 4 lines via \n, max_lines=2, no width constraint.
        let result = truncate_with_ellipsis(
            "Line1\nLine2\nLine3\nLine4",
            24.0,
            1.2,
            None,
            None,
            2,
            &mut fs,
        );
        assert!(result.content.ends_with('…'));
        assert!(result.content.contains("Line1"));
        assert!(result.content.contains("Line2"));
        assert!(!result.content.contains("Line3"));
        assert!(!result.content.contains("Line4"));
        assert_eq!(result.line_count, 2);
    }

    #[test]
    fn test_max_lines_zero_treated_as_one() {
        let mut fs = font_system();
        let result = truncate_with_ellipsis(
            "Hello World this is long",
            24.0,
            1.2,
            None,
            Some(50.0),
            0,
            &mut fs,
        );
        // max_lines=0 → treated as 1. Long text at 50px width → truncates.
        assert!(result.content.ends_with('…'));
        assert_eq!(result.line_count, 1);
    }

    #[test]
    fn test_empty_content_no_ellipsis() {
        let mut fs = font_system();
        let result = truncate_with_ellipsis("", 24.0, 1.2, None, Some(100.0), 2, &mut fs);
        assert_eq!(result.content, "");
        assert!(!result.content.ends_with('…'));
    }

    #[test]
    fn test_cjk_truncation() {
        let mut fs = font_system();
        // CJK characters have no spaces — truncation must work at glyph level.
        let result = truncate_with_ellipsis(
            "こんにちは世界これは長いテキストです",
            24.0,
            1.2,
            None,
            Some(80.0),
            1,
            &mut fs,
        );
        assert!(result.content.ends_with('…'));
        assert_eq!(result.line_count, 1);
        // Truncated content should be shorter than original.
        assert!(result.content.len() < "こんにちは世界これは長いテキストです".len() + 3);
    }

    #[test]
    fn test_exact_fit_no_ellipsis() {
        let mut fs = font_system();
        // 2 explicit lines, max_lines=2 — exact fit, no ellipsis.
        let result = truncate_with_ellipsis("Line1\nLine2", 24.0, 1.2, None, None, 2, &mut fs);
        assert!(!result.content.ends_with('…'));
        assert_eq!(result.content, "Line1\nLine2");
        assert_eq!(result.line_count, 2);
    }
}
