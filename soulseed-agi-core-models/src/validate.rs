use crate::{
    awareness::AwarenessCycleRecord,
    dialogue_event::DialogueEvent,
    message::Message,
    profiles::{AIProfile, HumanProfile},
    relationships::RelationshipSnapshot,
    session::Session,
    ModelError,
};

pub trait InvariantCheck {
    fn validate(&self) -> Result<(), ModelError>;
}

impl InvariantCheck for DialogueEvent {
    fn validate(&self) -> Result<(), ModelError> {
        DialogueEvent::validate(self)
    }
}

impl InvariantCheck for Message {
    fn validate(&self) -> Result<(), ModelError> {
        Message::validate(self)
    }
}

impl InvariantCheck for HumanProfile {
    fn validate(&self) -> Result<(), ModelError> {
        HumanProfile::validate(self)
    }
}

impl InvariantCheck for AIProfile {
    fn validate(&self) -> Result<(), ModelError> {
        AIProfile::validate(self)
    }
}

impl InvariantCheck for Session {
    fn validate(&self) -> Result<(), ModelError> {
        Session::validate(self)
    }
}

impl InvariantCheck for AwarenessCycleRecord {
    fn validate(&self) -> Result<(), ModelError> {
        AwarenessCycleRecord::validate(self)
    }
}

impl InvariantCheck for RelationshipSnapshot {
    fn validate(&self) -> Result<(), ModelError> {
        RelationshipSnapshot::validate(self)
    }
}
