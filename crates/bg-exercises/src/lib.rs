//! Exercise types. To add one: implement [`bg_ui::ExerciseType`] in a new module and
//! list it in [`all`].

pub mod basic_chords;
pub mod basic_timing;
pub mod timing_session;

use bg_ui::ExerciseType;

/// The registry shown in the sidebar, in display order.
pub fn all() -> Vec<Box<dyn ExerciseType>> {
    vec![
        Box::new(basic_timing::BasicTiming::default()),
        Box::new(basic_chords::BasicChords::default()),
    ]
}
