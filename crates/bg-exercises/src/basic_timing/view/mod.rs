//! Basic timing's view regions, one per place the shell gives it: the toolbar, the
//! notices below it, the scrolling main area and the results dialog. Each takes its
//! props and theme and sends messages; none changes the model.

pub mod main;
pub mod notices;
pub mod results;
pub mod status;
pub mod toolbar;
