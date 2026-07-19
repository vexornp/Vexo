//! Mocked IM UI — three-tab app shell (Chats / Contacts / Me) with
//! in-memory data, no network or persistence.

mod app;
mod chats;
mod contacts;
mod data;
mod me;
mod shadows;
mod widgets;

#[cfg(test)]
mod integration_tests;

pub use app::MobileApp;
pub use data::ImState;

uniffi::setup_scaffolding!();
