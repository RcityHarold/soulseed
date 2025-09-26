use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct ExtraVectors {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub emotion_vector: Option<Vec<f32>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prosody_vector: Option<Vec<f32>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub style_vector: Option<Vec<f32>>,
}
