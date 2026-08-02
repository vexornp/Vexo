//! TitledContainer — a desktop panel with a header (title + hairline) above
//! its content. Used by the desktop Chats columns 2/3 nav bars.

use vexo::{column, row, DecoratedBox, Layout, Style, Text, Widget, WithLayout};
use vexo_uikit::theme::tokens::navigation::{
    NavColors, HAIRLINE_THICKNESS, HEADER_FONT_SIZE, HEADER_PADDING, MOBILE_HEADER_HEIGHT,
};

/// Build a container with a titled header bar (left-aligned title on
/// `header_bg`) followed by a hairline divider, then the content child
/// filling the rest.
pub(crate) fn titled_container(
    title: impl Into<String>,
    child: Box<dyn Widget>,
    colors: &NavColors,
) -> Box<dyn Widget> {
    let header = DecoratedBox::with_style(
        WithLayout::new(
            Text::new(title.into().as_str())
                .with_font_size(HEADER_FONT_SIZE)
                .with_color(colors.header_text),
            Layout::default()
                .padding(HEADER_PADDING)
                .height(MOBILE_HEADER_HEIGHT)
                .flex_shrink(0.0),
        ),
        Style::default().background(colors.header_bg),
    );

    let hairline = DecoratedBox::with_style(
        row! {}.height(HAIRLINE_THICKNESS).flex_shrink(0.0),
        Style::default().background(colors.divider),
    );

    column! {
        header,
        hairline,
        WithLayout::new(child, Layout::flex_fill()),
    }
    .width_percent(1.0)
    .height_percent(1.0)
    .boxed()
}
