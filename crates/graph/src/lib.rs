//! The CPU side of the graphs panel: numeric series from a capture, their scales, how they are
//! stacked into bands, and how each is reduced to what fits in the pixels of a time window. No UI
//! code, so all of it is unit tested.

mod layout;
mod series;
mod ticks;
mod trace;

pub use layout::{Band, EntityBlock, Layout, Member, Stacking, layout};
pub use series::{GraphData, NameFilter, Scale, Series, Stats, depth};
pub use ticks::{format_time, format_value, time_ticks};
pub use trace::{Trace, trace};
