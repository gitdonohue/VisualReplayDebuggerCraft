//! The egui user interface for the replay debugger. It is shared by the desktop and web apps.
//!
//! The crate depends on `egui` only, not on a windowing backend, so the same code runs natively,
//! in the browser and in headless tests.

mod app;
mod entities;
mod loader;
mod log_filter;
mod logs;
mod properties;
mod theme;
mod transport;
mod viewport;

pub use app::ReplayApp;
pub use loader::{Inbox, Incoming};
pub use log_filter::LogFilter;
