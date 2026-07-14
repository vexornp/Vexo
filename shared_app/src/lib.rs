use std::any::Any;
use std::rc::Rc;

use vexo::{
    Application, Column, Component, ComponentState, DecoratedContainer, Flex, IndexedStack,
    LifecycleContext, RenderContext, Row, SafeArea, ScrollView, Signal, Text, TextEdit,
    TextEditingController, Theme, ThemeData, Widget,
};
use vexo_fontawesome::{Icon, Icons};
use vexo_uikit::{
    theme::tokens::navigation, Button, ButtonVariant, NavigationController, NavigationStackView,
    Platform,
};

uniffi::setup_scaffolding!();

/// Mobile navigation destinations: drilling into a sidebar item, or pushing
/// a numbered "page-N" page from a detail's "Next page" button.
#[derive(Hash, Eq, PartialEq, Clone, Debug)]
enum Dest {
    Item(&'static str),
    Page(u32),
}

const ITEMS: &[(&str, &str)] = &[
    ("inbox", "Inbox"),
    ("starred", "Starred"),
    ("sent", "Sent"),
    ("drafts", "Drafts"),
    ("archive", "Archive"),
    ("trash", "Trash"),
];

fn item_label(id: &str) -> String {
    ITEMS
        .iter()
        .find(|(i, _)| *i == id)
        .map(|(_, l)| l.to_string())
        .unwrap_or_else(|| id.to_string())
}

/// Map the desktop sidebar selection to an `IndexedStack` child index.
/// Falls back to `0` (Inbox) if `selected` is `None` — unreachable on
/// desktop in practice (`new()` sets `Some("inbox")`, sidebar only ever
/// sets `Some(id)`), but defensive.
fn selected_index(selected: Option<&'static str>) -> usize {
    selected
        .and_then(|id| ITEMS.iter().position(|(i, _)| *i == id))
        .unwrap_or(0)
}

#[derive(ComponentState)]
pub struct State {
    selection_log: Signal<u32>,
    is_dark: Signal<bool>,
    /// Desktop sidebar selection (mobile uses the nav stack for everything).
    selected: Signal<Option<&'static str>>,
    /// Desktop: one controller per sidebar item, indexed by `ITEMS` position.
    /// Each item's nav stack persists across sidebar toggles because the
    /// corresponding `NavigationStackView` stays mounted inside the
    /// `IndexedStack` (wrapped in `Offstage`).
    nav_controllers: Vec<NavigationController<Dest>>,
    /// Mobile: single shared nav stack. Semantically distinct from desktop's
    /// per-item stacks; must persist in `State` (not be created per `view()`)
    /// because `NavigationStackView`'s `on_mount` wires its dirty callback and
    /// its path must survive across rebuilds.
    mobile_nav_controller: NavigationController<Dest>,
}

/// Manual `Default` (replacing `#[derive(Default)]`) because the desktop path
/// constructs `State` via `StatefulElement::mount()` → `W::State::default()`
/// (`vexo/src/stateful_widget.rs:537`), NOT via `Application::new()`. The
/// backfill of `nav_controllers` to `ITEMS.len()` must happen here, or
/// `view()`'s `state.nav_controllers[i]` indexing panics on the first frame.
/// `#[derive(ComponentState)]` is unaffected — it only wires `Signal` fields.
impl Default for State {
    fn default() -> Self {
        let mut nav_controllers = Vec::new();
        while nav_controllers.len() < ITEMS.len() {
            nav_controllers.push(NavigationController::new());
        }
        Self {
            selection_log: Signal::new(0),
            is_dark: Signal::new(false),
            selected: Signal::new(None),
            nav_controllers,
            mobile_nav_controller: NavigationController::new(),
        }
    }
}

impl Application for State {
    type State = Self;

    fn new() -> Self::State {
        let state = Self::State::default();
        state.selected.set(Some("inbox"));
        state
    }

    fn register_fonts(font_system: &mut glyphon::FontSystem) {
        vexo_fontawesome::register_fonts(font_system);
    }

    fn view(state: &mut Self::State) -> Box<dyn Widget> {
        let selected_signal = state.selected.clone();
        let selection_count = state.selection_log.clone();
        let is_dark = state.is_dark.get();
        let theme = if is_dark {
            ThemeData::dark()
        } else {
            ThemeData::light()
        };
        let is_dark_signal = state.is_dark.clone();

        let inner: Box<dyn Widget> = match Platform::current() {
            Platform::Desktop => {
                let current = selected_signal.get_cloned();
                let index = selected_index(current);

                let selected_for_cb = selected_signal.clone();
                let sidebar = build_sidebar(
                    current,
                    Rc::new(move |id| {
                        selected_for_cb.set(Some(id));
                    }),
                    false,
                    theme.clone(),
                    is_dark_signal.clone(),
                );

                let mut stack = IndexedStack::new(index);
                for (i, (id, label)) in ITEMS.iter().enumerate() {
                    let ctrl = state.nav_controllers[i].clone();
                    let detail = build_detail_content(id, selection_count.clone(), ctrl.clone());
                    let nav_for_dest = ctrl.clone();
                    stack = stack.push(
                        NavigationStackView::new(ctrl, detail)
                            .root_title(label.to_string())
                            .title(|d| match d {
                                Dest::Page(n) => format!("Page: {}", n),
                                _ => String::new(),
                            })
                            .destination(move |d| match d {
                                Dest::Page(n) => PageContent {
                                    n: *n,
                                    nav_controller: nav_for_dest.clone(),
                                }
                                .boxed(),
                                _ => Text::new("").boxed(),
                            })
                            .boxed(),
                    );
                }

                SafeArea::new(
                    Flex::row()
                        .flex_grow(1.0)
                        .background(theme.background)
                        .push(sidebar)
                        .push(stack.flex_grow(1.0)),
                )
                .boxed()
            }
            Platform::Mobile => {
                let nav_for_select = state.mobile_nav_controller.clone();
                let sidebar = build_sidebar(
                    None,
                    Rc::new(move |id| {
                        nav_for_select.push(Dest::Item(id));
                    }),
                    true,
                    theme.clone(),
                    is_dark_signal.clone(),
                );

                let nav_for_dest = state.mobile_nav_controller.clone();
                let count_for_dest = selection_count.clone();

                NavigationStackView::new(state.mobile_nav_controller.clone(), sidebar)
                    .root_title("Navigation")
                    .title(|d| match d {
                        Dest::Item(id) => item_label(*id),
                        Dest::Page(n) => format!("Page: {}", n),
                    })
                    .destination(move |d| match d {
                        Dest::Item(id) => {
                            build_detail_content(*id, count_for_dest.clone(), nav_for_dest.clone())
                        }
                        Dest::Page(n) => PageContent {
                            n: *n,
                            nav_controller: nav_for_dest.clone(),
                        }
                        .boxed(),
                    })
                    .boxed()
            }
        };

        Theme::new(theme, inner).boxed()
    }
}

fn build_sidebar(
    selected: Option<&str>,
    on_select: Rc<dyn Fn(&'static str)>,
    full_width: bool,
    theme: ThemeData,
    is_dark: Signal<bool>,
) -> Box<dyn Widget> {
    let nav = navigation::colors(&theme);
    let dark = is_dark.get();

    // Icon shows the TARGET mode (tap to go there): moon when light, sun when dark.
    let (icon, target_label) = if dark {
        (Icons::Sun, "Light")
    } else {
        (Icons::Moon, "Dark")
    };
    let icon_color = theme.on_surface;
    let toggle_is_dark = is_dark.clone();

    let toggle_button =
        DecoratedContainer::new(Icon::new(icon).with_size(20.0).with_color(icon_color))
            .padding(8.0)
            .boxed()
            .on_press(move || {
                toggle_is_dark.set(!toggle_is_dark.get());
            });

    let header = Flex::row()
        .padding(12.0)
        .background(nav.header_bg)
        .push(
            Text::new("Navigation")
                .with_font_size(navigation::HEADER_FONT_SIZE)
                .with_color(nav.header_text),
        )
        .push(Flex::new().flex_grow(1.0))
        .push(toggle_button)
        .boxed();

    let mut list = Flex::column();
    // Mobile: no header, so prepend a toggle row to the list.
    // Styled like build_item_row but with an icon + label (spec: "icon + label").
    if full_width {
        let row_is_dark = is_dark.clone();
        let toggle_content = Row::new()
            .gap(8.0)
            .push(Icon::new(icon).with_size(16.0).with_color(nav.row_text))
            .push(
                Text::new(target_label)
                    .with_font_size(navigation::ROW_FONT_SIZE)
                    .with_color(nav.row_text),
            );
        let toggle_row = DecoratedContainer::new(toggle_content)
            .background(nav.row_bg)
            .padding(navigation::ROW_PADDING)
            .boxed()
            .on_press(move || {
                row_is_dark.set(!row_is_dark.get());
            });
        list = list.push(toggle_row);
    }
    for &(id, label) in ITEMS {
        let is_selected = selected == Some(id);
        let on_select = on_select.clone();
        let row = build_item_row(label, is_selected, move || on_select(id), &nav);
        list = list.push(row);
    }

    let mut sidebar = Flex::column().background(nav.sidebar_bg);
    if full_width {
        sidebar = sidebar.flex_grow(1.0);
    } else {
        sidebar = sidebar.width(240.0).flex_shrink(0.0);
        sidebar = sidebar.push(header);
    }
    sidebar
        .push(ScrollView::new(list.boxed()).flex_grow(1.0))
        .boxed()
}

fn build_item_row(
    label: &str,
    is_selected: bool,
    on_press: impl FnMut() + 'static,
    nav: &navigation::NavColors,
) -> Box<dyn Widget> {
    let text_color = if is_selected {
        nav.selected_text
    } else {
        nav.row_text
    };
    let bg = if is_selected {
        nav.selected_bg
    } else {
        nav.row_bg
    };

    let label_text = Text::new(label)
        .with_font_size(navigation::ROW_FONT_SIZE)
        .with_color(text_color);

    DecoratedContainer::new(label_text)
        .background(bg)
        .padding(navigation::ROW_PADDING)
        .boxed()
        .on_press(on_press)
}

fn build_detail_content(
    id: &str,
    selection_count: Signal<u32>,
    nav_controller: NavigationController<Dest>,
) -> Box<dyn Widget> {
    DetailPage {
        id: id.to_string(),
        selection_count,
        nav_controller,
    }
    .boxed()
}

// ============================================================================
// DETAIL PAGE COMPONENT
// ============================================================================

/// Detail page for a sidebar item. Each instance owns its own
/// `TextEditingController` (for the "inbox" text-edit showcase), created on
/// mount and dropped on unmount. This means every push to a fresh detail page
/// starts with the original text, and edits do not leak across push/pop
/// cycles — the bug that occurred when a single shared controller was reused.
struct DetailPage {
    id: String,
    selection_count: Signal<u32>,
    nav_controller: NavigationController<Dest>,
}

impl Clone for DetailPage {
    fn clone(&self) -> Self {
        Self {
            id: self.id.clone(),
            selection_count: self.selection_count.clone(),
            nav_controller: self.nav_controller.clone(),
        }
    }
}

/// State for `DetailPage`. The `TextEditingController` is created in
/// `on_mount` (not `Default`) because construction needs a `FontSystem`, and
/// because it must be scoped to this element's lifetime — fresh on mount,
/// dropped on unmount.
///
/// The framework does not expose the window's `FontSystem` in
/// `LifecycleContext`, so we construct a throwaway one here solely for the
/// initial `set_text`/`shape_until_scroll`. This matches the prior
/// `demo_text_controller()` singleton approach. The initial text is ASCII,
/// fully covered by the embedded Roboto font; subsequent typing uses the real
/// window `FontSystem` via `EventContext` during `on_event`.
#[derive(Default)]
struct DetailPageState {
    text_controller: Option<TextEditingController>,
}

impl DetailPageState {
    /// (Re)initialize the text controller for the current `id`.
    ///
    /// Called from `on_mount` (fresh element) and from `on_update` when the
    /// `id` changes (element reused across sidebar items via type-only
    /// `can_update`). Drops any stale controller first so edits never leak
    /// across logical pages sharing one element.
    fn sync_controller(&mut self, id: &str) {
        self.text_controller = None;
        if id == "inbox" {
            let mut font_system = vexo::resource::new_font_system();
            self.text_controller = Some(TextEditingController::new(
                "Hello, edit me! Try Cmd+A, Cmd+C, Cmd+V.",
                &mut font_system,
            ));
        }
    }
}

impl ComponentState for DetailPageState {
    fn on_mount(&mut self, ctx: &mut LifecycleContext) {
        if let Some(page) = ctx.widget().downcast_ref::<DetailPage>() {
            self.sync_controller(&page.id);
        }
    }

    fn on_update(&mut self, old_widget: &dyn Any, ctx: &mut LifecycleContext) {
        // The framework reconciles `DetailPage` by type only, so the same
        // element (and its state) is reused when the sidebar selection
        // changes (e.g. starred → inbox). `on_mount` does not re-run in that
        // case, so we must re-sync the controller here whenever the `id`
        // changes — otherwise an inbox render would hit a stale `None`
        // controller left over from a non-inbox page.
        let old_id = old_widget
            .downcast_ref::<DetailPage>()
            .map(|p| p.id.as_str());
        let new_id = ctx
            .widget()
            .downcast_ref::<DetailPage>()
            .map(|p| p.id.as_str());
        if old_id != new_id {
            if let Some(new_id) = new_id {
                self.sync_controller(new_id);
            }
        }
    }

    fn on_unmount(&mut self, _ctx: &mut LifecycleContext) {
        // Drop the controller. Its dirty callback was already cleared by
        // TextEdit's on_unmount (children unmount after parent's on_unmount
        // but before parent state is dropped).
        self.text_controller = None;
    }
}

impl Component for DetailPage {
    type State = DetailPageState;

    fn render(&self, state: &mut Self::State, ctx: &mut RenderContext) -> Box<dyn Widget> {
        let theme = Theme::of(ctx);
        let title_widget = Text::new(self.id.as_str())
            .with_font_size(32.0)
            .with_color(theme.on_background);

        let body: Box<dyn Widget> = if self.id == "inbox" {
            let controller = state
                .text_controller
                .as_ref()
                .expect("inbox DetailPage must have a controller after on_mount")
                .clone();
            Column::new()
                .gap(8.0)
                .push(
                    Row::new()
                        .gap(8.0)
                        .push(
                            Icon::new(Icons::FloppyDisk)
                                .with_size(24.0)
                                .with_color(theme.on_background),
                        )
                        .push(
                            Text::new("Text Edit Showcase")
                                .with_font_size(24.0)
                                .with_color(theme.on_background),
                        ),
                )
                .push(TextEdit::new(controller))
                .boxed()
        } else {
            Column::new()
                .push(
                    Text::new(format!("This is the detail content for \"{}\".", self.id))
                        .with_color(theme.on_background),
                )
                .boxed()
        };

        let count = self.selection_count.clone();
        let root_nav = self.nav_controller.clone();
        Column::new()
            .gap(16.0)
            .padding(24.0)
            .background(theme.background)
            .push(title_widget)
            .push(body)
            .push(
                Button::new("Bump counter")
                    .variant(ButtonVariant::Primary)
                    .on_press(move || {
                        count.set(count.get() + 1);
                    }),
            )
            .push(
                Text::new(format!("Counter: {}", self.selection_count.get()))
                    .with_color(theme.on_background),
            )
            .push(
                Button::new("Next page")
                    .variant(ButtonVariant::Primary)
                    .on_press(move || {
                        root_nav.push(Dest::Page(1));
                    }),
            )
            .boxed()
    }
}

// ============================================================================
// PAGE CONTENT COMPONENT
// ============================================================================

/// Pushed page content. Implemented as a `Component` (not a free function)
/// so it establishes an inherited-widget dependency via `Theme::of(ctx)` and
/// auto-rebuilds when the theme toggles after the page has been pushed.
#[derive(Default)]
struct PageContentState;

impl ComponentState for PageContentState {}

struct PageContent {
    n: u32,
    nav_controller: NavigationController<Dest>,
}

impl Clone for PageContent {
    fn clone(&self) -> Self {
        Self {
            n: self.n,
            nav_controller: self.nav_controller.clone(),
        }
    }
}

impl Component for PageContent {
    type State = PageContentState;

    fn render(&self, _state: &mut Self::State, ctx: &mut RenderContext) -> Box<dyn Widget> {
        let theme = Theme::of(ctx);
        let ctrl = self.nav_controller.clone();
        let n = self.n;
        Column::new()
            .gap(16.0)
            .padding(24.0)
            .background(theme.background)
            .push(
                Text::new(format!("Page: {}", n))
                    .with_font_size(24.0)
                    .with_color(theme.on_background),
            )
            .push(
                Text::new(format!("You are on pushed page \"{}\".", n))
                    .with_color(theme.on_background),
            )
            .push(
                Button::new("Next page")
                    .variant(ButtonVariant::Primary)
                    .on_press(move || {
                        ctrl.push(Dest::Page(n + 1));
                    }),
            )
            .boxed()
    }
}

#[derive(uniffi::Object)]
pub struct MobileApp {}

#[uniffi::export]
impl MobileApp {
    #[uniffi::constructor]
    pub fn new() -> Self {
        Self {}
    }

    pub fn start_app(&self) {
        let rt = vexo::run_desktop_demo::<State>();
        match rt {
            Ok(_) => println!("App exited normally"),
            Err(e) => println!("App exited with error: {:?}", e),
        }
    }
}
