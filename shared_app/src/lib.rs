//! Mocked IM UI — three-tab app shell (Chats / Contacts / Me) with
//! in-memory data, no network or persistence.

mod app;
mod chats;
mod contacts;
mod data;
mod me;
mod widgets;

#[cfg(not(any(target_os = "ios", target_os = "android")))]
mod desktop_shell;

#[cfg(test)]
mod integration_tests;

pub use app::MobileApp;
pub use data::ImState;

uniffi::setup_scaffolding!();
