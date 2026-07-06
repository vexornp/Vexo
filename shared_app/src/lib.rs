use std::rc::Rc;

use vexo::{
    Application, Color, Column, ComponentState, DecoratedContainer, Flex, Row, SafeArea,
    ScrollView, Signal, Text, TextEdit, TextEditingController, Widget,
};
use vexo_fontawesome::{Icon, Icons};
use vexo_uikit::{Button, ButtonVariant, NavigationController, NavigationStackView, Platform};

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

#[derive(ComponentState, Default)]
pub struct State {
    selection_log: Signal<u32>,
    /// Desktop sidebar selection (mobile uses the nav stack for everything).
    selected: Signal<Option<&'static str>>,
    nav_controller: NavigationController<Dest>,
}

thread_local! {
    static TEXT_CONTROLLER: std::cell::RefCell<Option<TextEditingController>> =
        std::cell::RefCell::new(None);
}

fn demo_text_controller() -> TextEditingController {
    TEXT_CONTROLLER.with(|c| {
        let mut c = c.borrow_mut();
        if c.is_none() {
            let mut font_system = vexo::resource::new_font_system();
            *c = Some(TextEditingController::new(
                "Hello, edit me! Try Cmd+A, Cmd+C, Cmd+V.",
                &mut font_system,
            ));
        }
        c.as_ref().unwrap().clone()
    })
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
        let nav_controller = state.nav_controller.clone();
        let selection_count = state.selection_log.clone();

        match Platform::current() {
            Platform::Desktop => {
                let current = selected_signal.get_cloned();

                let selected_for_cb = selected_signal.clone();
                let nav_for_cb = nav_controller.clone();
                let sidebar = build_sidebar(
                    current,
                    Rc::new(move |id| {
                        selected_for_cb.set(Some(id));
                        nav_for_cb.pop_to_root();
                    }),
                    false,
                );

                let detail_root = match current {
                    Some(id) => {
                        build_detail_content(id, selection_count.clone(), nav_controller.clone())
                    }
                    None => Text::new("Select an item").boxed(),
                };
                let root_title = current
                    .as_ref()
                    .map(|id| item_label(id))
                    .unwrap_or_default();

                let nav_for_dest = nav_controller.clone();
                let detail = NavigationStackView::new(nav_controller, detail_root)
                    .root_title(root_title)
                    .title(|d| match d {
                        Dest::Page(n) => format!("Page: {}", n),
                        _ => String::new(),
                    })
                    .destination(move |d| match d {
                        Dest::Page(n) => build_page_content(*n, nav_for_dest.clone()),
                        _ => Text::new("").boxed(),
                    })
                    .boxed()
                    .flex_grow(1.0);

                SafeArea::new(
                    Flex::row()
                        .background(Color::WHITE)
                        .push(sidebar)
                        .push(detail),
                )
                .boxed()
            }
            Platform::Mobile => {
                let nav_for_select = nav_controller.clone();
                let sidebar = build_sidebar(
                    None,
                    Rc::new(move |id| {
                        nav_for_select.push(Dest::Item(id));
                    }),
                    true,
                );

                let nav_for_dest = state.nav_controller.clone();
                let count_for_dest = selection_count.clone();

                SafeArea::new(
                    NavigationStackView::new(state.nav_controller.clone(), sidebar)
                        .root_title("Navigation")
                        .title(|d| match d {
                            Dest::Item(id) => item_label(*id),
                            Dest::Page(n) => format!("Page: {}", n),
                        })
                        .destination(move |d| match d {
                            Dest::Item(id) => build_detail_content(
                                *id,
                                count_for_dest.clone(),
                                nav_for_dest.clone(),
                            ),
                            Dest::Page(n) => build_page_content(*n, nav_for_dest.clone()),
                        }),
                )
                .boxed()
            }
        }
    }
}

fn build_sidebar(
    selected: Option<&str>,
    on_select: Rc<dyn Fn(&'static str)>,
    full_width: bool,
) -> Box<dyn Widget> {
    let header = Flex::row()
        .padding(12.0)
        .background(Color::rgb(0.9, 0.9, 0.92))
        .push(
            Text::new("Navigation")
                .with_font_size(16.0)
                .with_color(Color::rgb(0.2, 0.2, 0.2)),
        )
        .boxed();

    let mut list = Flex::column();
    for &(id, label) in ITEMS {
        let is_selected = selected == Some(id);
        let on_select = on_select.clone();
        let row = build_item_row(label, is_selected, move || on_select(id));
        list = list.push(row);
    }

    let mut sidebar = Flex::column().background(Color::rgb(0.95, 0.95, 0.97));
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
) -> Box<dyn Widget> {
    let text_color = if is_selected {
        Color::WHITE
    } else {
        Color::rgb(0.1, 0.1, 0.1)
    };
    let bg = if is_selected {
        Color::rgb(0.0, 0.478, 1.0)
    } else {
        Color::TRANSPARENT
    };

    let label_text = Text::new(label).with_font_size(16.0).with_color(text_color);

    DecoratedContainer::new(label_text)
        .background(bg)
        .padding(10.0)
        .boxed()
        .on_press(on_press)
}

fn build_detail_content(
    id: &str,
    selection_count: Signal<u32>,
    nav_controller: NavigationController<Dest>,
) -> Box<dyn Widget> {
    let title_widget = Text::new(id).with_font_size(32.0);

    let body: Box<dyn Widget> = if id == "inbox" {
        Column::new()
            .gap(8.0)
            .push(
                Row::new()
                    .gap(8.0)
                    .push(
                        Icon::new(Icons::FloppyDisk)
                            .with_size(24.0)
                            .with_color(Color::BLACK),
                    )
                    .push(Text::new("Text Edit Showcase").with_font_size(24.0)),
            )
            .push(TextEdit::new(demo_text_controller()))
            .boxed()
    } else {
        Column::new()
            .push(Text::new(format!(
                "This is the detail content for \"{}\".",
                id
            )))
            .boxed()
    };

    let count = selection_count.clone();
    let root_nav = nav_controller.clone();
    Column::new()
        .gap(16.0)
        .padding(24.0)
        .background(Color::WHITE)
        .push(title_widget)
        .push(body)
        .push(
            Button::new("Bump counter")
                .variant(ButtonVariant::Primary)
                .on_press(move || {
                    count.set(count.get() + 1);
                }),
        )
        .push(Text::new(format!("Counter: {}", selection_count.get())))
        .push(
            Button::new("Next page")
                .variant(ButtonVariant::Primary)
                .on_press(move || {
                    root_nav.push(Dest::Page(1));
                }),
        )
        .boxed()
}

fn build_page_content(n: u32, nav_controller: NavigationController<Dest>) -> Box<dyn Widget> {
    let ctrl = nav_controller.clone();
    Column::new()
        .gap(16.0)
        .padding(24.0)
        .push(Text::new(format!("Page: {}", n)).with_font_size(24.0))
        .push(Text::new(format!("You are on pushed page \"{}\".", n)))
        .push(
            Button::new("Next page")
                .variant(ButtonVariant::Primary)
                .on_press(move || {
                    ctrl.push(Dest::Page(n + 1));
                }),
        )
        .boxed()
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
