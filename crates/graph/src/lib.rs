//! The CPU side of the time-based panels. For the graphs: numeric series from a capture, their
//! scales, how they are stacked into bands, and how each is reduced to what fits in the pixels of a
//! time window. For the properties timelines: string parameters as runs of values, split by the
//! levels of their paths, and the blocks to draw for a window. For the entity timelines: when each
//! entity lived and logged. No UI code, so all of it is unit tested.

mod layout;
mod lifespans;
mod rows;
mod series;
mod ticks;
mod trace;
mod tracks;

pub use layout::{Band, EntityBlock, Layout, Member, Stacking, layout};
pub use lifespans::{Lifespan, event_columns, lifespans, span_columns};
pub use rows::{
    Block, BlockKind, MIN_BLOCK_PX, Row, RowOptions, Section, blocks, event_times, frame_stripes, neighbouring_event,
    row_level, rows, short_names, smallest_block_at,
};
pub use series::{GraphData, NameFilter, Scale, Series, Stats, depth};
pub use ticks::{format_time, format_value, time_ticks};
pub use trace::{Trace, trace};
pub use tracks::{Level, Run, TimelineData, Track, color_label, path_of, segments};
