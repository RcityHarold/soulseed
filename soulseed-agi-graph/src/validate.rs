use crate::{AccessClass, DialogueEvent, GraphError};

pub trait InvariantCheck {
    fn validate(&self) -> Result<(), GraphError>;
}

impl InvariantCheck for DialogueEvent {
    fn validate(&self) -> Result<(), GraphError> {
        if self.sequence_number == 0 {
            return Err(GraphError::InvalidQuery("sequence_number must be >= 1"));
        }
        if self.timestamp_ms < 0 {
            return Err(GraphError::InvalidQuery("timestamp must be non-negative"));
        }
        if matches!(self.access_class, AccessClass::Restricted) && self.provenance.is_none() {
            return Err(GraphError::PrivacyRestricted);
        }
        if let Some(meta) = &self.embedding_meta {
            if let Some(vec) = &self.content_embedding {
                if vec.len() as u16 != meta.dim {
                    return Err(GraphError::InvalidQuery("content embedding dim mismatch"));
                }
            }
            if let Some(vec) = &self.context_embedding {
                if vec.len() as u16 != meta.dim {
                    return Err(GraphError::InvalidQuery("context embedding dim mismatch"));
                }
            }
            if let Some(vec) = &self.decision_embedding {
                if vec.len() as u16 != meta.dim {
                    return Err(GraphError::InvalidQuery("decision embedding dim mismatch"));
                }
            }
            if let Some(vec) = &self.concept_vector {
                if vec.len() as u16 != meta.dim {
                    return Err(GraphError::InvalidQuery("concept vector dim mismatch"));
                }
            }
        }
        Ok(())
    }
}
