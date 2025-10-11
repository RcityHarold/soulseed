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

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum MembershipLevel {
    Free,
    Pro,
    Premium,
    Ultimate,
    Team,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum SubscriptionStatus {
    None,
    Trialing,
    Active,
    Paused,
    Cancelled,
    Expired,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum RelationshipStatus {
    Stranger,
    Acquaintance,
    Friend,
    CloseFriend,
    Companion,
    Blocked,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CorePersonalityKind {
    Gentle,
    Rational,
    Explorer,
    Steady,
    Uplifting,
    Imaginative,
    Disciplined,
}

impl CorePersonalityKind {
    pub const fn code(&self) -> &'static str {
        match self {
            Self::Gentle => "GT",
            Self::Rational => "RT",
            Self::Explorer => "ET",
            Self::Steady => "ST",
            Self::Uplifting => "UT",
            Self::Imaginative => "IT",
            Self::Disciplined => "DT",
        }
    }

    pub const fn display_name(&self) -> &'static str {
        match self {
            Self::Gentle => "温柔型 · 倾听与包覆之光",
            Self::Rational => "理性型 · 清晰与逻辑之光",
            Self::Explorer => "探索型 · 风与提问之光",
            Self::Steady => "沉稳型 · 大地与静默之光",
            Self::Uplifting => "光辉型 · 鼓舞与明亮之光",
            Self::Imaginative => "灵感型 · 想象与比喻之光",
            Self::Disciplined => "自律型 · 精准与分寸之光",
        }
    }
}
