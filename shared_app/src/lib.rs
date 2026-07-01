use vexo::{
    Application, Color, Column, ComponentState, Signal, Text, TextEdit, TextEditingController,
    Widget,
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
            let mut font_system = glyphon::FontSystem::new();
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

            // The "Inbox" detail embeds the text-edit showcase so it stays
            // exercisable from this demo.
            let mut col = Column::new()
                .gap(16.0)
                .padding(24.0)
                .background(Color::WHITE)
                .push(title_widget);

            if *id == "inbox" {
                col = col
                    .push(Text::new("Text Edit Showcase").with_font_size(24.0))
                    .push(TextEdit::new(demo_text_controller()));
            } else {
                col = col.push(Text::new(format!(
                    "This is the detail content for \"{}\".",
                    id
                )));
            }

            // A button that bumps a counter, demonstrating state flow from
            // the detail pane back to the application.
            let count = selection_count.clone();
            col = col.push(
                Button::new("Bump counter")
                    .variant(ButtonVariant::Primary)
                    .on_press(move || {
                        count.set(count.get() + 1);
                    }),
            );

            let count_now = selection_count.get();
            col = col.push(Text::new(format!("Counter: {}", count_now)));

            col.boxed()
        };

        NavigationSplitView::new(items)
            .default_selection("inbox")
            .detail(detail_closure)
            .on_selection_change(|id| {
                log::debug!("NavigationSplitView selection changed: {}", id);
            })
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
