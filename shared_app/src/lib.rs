//! Mocked IM UI — three-tab app shell (Chats / Contacts / Me) with
//! in-memory data, no network or persistence.

mod app;
mod chats;
mod contacts;
mod data;
mod me;
mod widgets;

#[cfg(test)]
mod integration_tests;

pub use app::{ImState, MobileApp};

uniffi::setup_scaffolding!();
