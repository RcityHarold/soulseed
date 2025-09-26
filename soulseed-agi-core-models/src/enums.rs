use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum ConversationScenario {
    HumanToHuman,
    HumanGroup,
    HumanToAi,
    AiToAi,
    AiSelfTalk,
    HumanToMultiAi,
    MultiHumanToMultiAi,
    AiGroup,
    AiToSystem,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum DialogueEventType {
    Message,
    ToolCall,
    ToolResult,
    SelfReflection,
    Decision,
    Lifecycle,
    System,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum RealTimePriority {
    Low,
    Normal,
    High,
    Critical,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum SoulState {
    Activating,
    Active,
    Dormant,
}
