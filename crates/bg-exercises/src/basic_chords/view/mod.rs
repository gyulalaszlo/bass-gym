//! Basic chords' view regions, one per place the shell gives it: the toolbar, the
//! notices below it, the scrolling exercise area (live part, then settings) and the
//! results dialog. Each takes its props and theme and sends messages; none changes the
//! model. The other files are views the regions use.

pub mod config;
pub mod live;
pub mod notices;
pub mod per_chord;
pub mod results;
pub mod status;
pub mod strip;
pub mod toolbar;
