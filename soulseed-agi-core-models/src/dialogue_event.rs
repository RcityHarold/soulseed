pub mod payload;
pub mod v2;

pub use payload::*;
pub use v2::*;

pub use crate::legacy::dialogue_event::DialogueEvent as LegacyDialogueEvent;

use crate::ModelError;

pub fn validate_dialogue_event(event: &DialogueEvent) -> Result<(), ModelError> {
    event.validate()
}

pub fn convert_legacy_dialogue_event(event: LegacyDialogueEvent) -> DialogueEvent {
    DialogueEvent::from(event)
}
