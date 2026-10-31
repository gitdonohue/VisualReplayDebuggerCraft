//! The egui user interface for the replay debugger. It is shared by the desktop and web apps.
//!
//! The crate depends on `egui` only, not on a windowing backend, so the same code runs natively,
//! in the browser and in headless tests.

mod app;
mod entities;
mod graphs;
mod loader;
mod log_filter;
mod logs;
mod pins;
mod properties;
mod theme;
mod timeaxis;
mod timelines;
mod transport;
mod viewport;
mod widgets;

pub use app::{BottomTab, ReplayApp};
pub use graphs::GraphView;
pub use loader::{Inbox, Incoming};
pub use log_filter::LogFilter;
pub use pins::Pins;
pub use timelines::{HEADER_HEIGHT, ROW_HEIGHT, TimelineView};
