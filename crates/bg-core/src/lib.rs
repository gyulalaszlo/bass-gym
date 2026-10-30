//! Pure, I/O-light domain logic for bass-gym: musical time, grooves, session
//! configuration, bar planning, scoring and versioned presets.

pub mod groove;
pub mod harmony;
pub mod music;
pub mod plan;
pub mod preset;
pub mod scoring;
pub mod session;

pub use groove::{Groove, GrooveNote};
pub use music::{PPQ, TimeSignature};
pub use plan::{BarPlan, ClickKind, Planner};
pub use scoring::{Grade, GradeMode, Matcher, ScoringThresholds, SessionStats};
pub use session::SessionConfig;
