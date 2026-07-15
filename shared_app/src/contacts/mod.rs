//! Contacts tab: contact list wired into a NavigationStackView.

pub(crate) mod contacts_screen;

use vexo::Widget;
use vexo_uikit::{NavigationController, NavigationStackView};

use crate::data::Contact;

pub(crate) fn build_contacts_tab(
    contacts: Vec<Contact>,
    nav: NavigationController<()>,
) -> Box<dyn Widget> {
    NavigationStackView::new(nav, contacts_screen::build_contacts_screen(contacts))
        .root_title("Contacts")
        .boxed()
}
