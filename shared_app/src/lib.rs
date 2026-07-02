use vexo::{
    Application, Color, Column, ComponentState, SafeArea, Signal, Text, TextEdit,
    TextEditingController, Widget,
};
use vexo_uikit::{Button, ButtonVariant, NavigationItem, NavigationSplitView};

uniffi::setup_scaffolding!();

#[derive(ComponentState, Default)]
pub struct State {
    selection_log: Signal<u32>,
}

/// Lazily create a `TextEditingController` for the demo's TextEdit.
///
/// `TextEditingController::new` needs a `FontSystem` for initial text shaping,
/// but `Application::view()` doesn't receive one. We use a `thread_local!` to
/// create the controller exactly once on the main thread (with its own
/// throwaway `FontSystem`); subsequent calls cheaply clone the `Rc<RefCell<Editor>>`.
/// All later editing operations use the pipeline's `FontSystem` via `ctx.font_system`.
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
        Self::State::default()
    }

    fn view(state: &mut Self::State) -> Box<dyn Widget> {
        let items = vec![
            NavigationItem::new("inbox", "Inbox"),
            NavigationItem::new("starred", "Starred"),
            NavigationItem::new("sent", "Sent"),
            NavigationItem::new("drafts", "Drafts"),
            NavigationItem::new("archive", "Archive"),
            NavigationItem::new("trash", "Trash"),
        ];

        let selection_count = state.selection_log.clone();
        let detail_closure = move |id: &&str| -> Box<dyn Widget> {
            let title_widget = Text::new(*id).with_font_size(32.0);

            // Page-specific body, wrapped in a Column so slot 1 is always the
            // same widget type (Column → ContainerElement) across all pages.
            // This avoids positional-reconciliation type mismatches: when
            // switching pages, slot 1's element is updated in place rather
            // than being force-updated with an incompatible widget type.
            let body: Box<dyn Widget> = if *id == "inbox" {
                Column::new()
                    .gap(8.0)
                    .push(Text::new("Text Edit Showcase").with_font_size(24.0))
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
            Column::new()
                .gap(16.0)
                .padding(24.0)
                .background(Color::WHITE)
                .push(title_widget) // slot 0: always Text
                .push(body) // slot 1: always Column
                .push(
                    Button::new("Bump counter") // slot 2: always Button
                        .variant(ButtonVariant::Primary)
                        .on_press(move || {
                            count.set(count.get() + 1);
                        }),
                )
                .push(Text::new(format!(
                    // slot 3: always Text
                    "Counter: {}",
                    selection_count.get()
                )))
                .boxed()
        };

        SafeArea::new(
            NavigationSplitView::new(items)
                .default_selection("inbox")
                .detail(detail_closure)
                .on_selection_change(|id| {
                    log::debug!("NavigationSplitView selection changed: {}", id);
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
