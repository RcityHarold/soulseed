use crate::types::SemanticEdgeKind;
use crate::{ConversationScenario, DialogueEventType};

#[derive(Clone, Debug)]
pub struct ScenarioRule {
    pub scenario: ConversationScenario,
    pub description: &'static str,
    pub primary_event_types: &'static [DialogueEventType],
    pub allowed_semantic_edges: &'static [SemanticEdgeKind],
}

const HUMAN_TO_AI_EVENTS: &[DialogueEventType] = &[
    DialogueEventType::Message,
    DialogueEventType::ToolCall,
    DialogueEventType::ToolResult,
    DialogueEventType::Decision,
    DialogueEventType::Lifecycle,
];

const HUMAN_GROUP_EVENTS: &[DialogueEventType] = &[
    DialogueEventType::Message,
    DialogueEventType::Lifecycle,
    DialogueEventType::System,
];

const AI_TO_AI_EVENTS: &[DialogueEventType] = &[
    DialogueEventType::Message,
    DialogueEventType::Decision,
    DialogueEventType::ToolCall,
    DialogueEventType::ToolResult,
];

const AI_SELF_TALK_EVENTS: &[DialogueEventType] = &[
    DialogueEventType::Message,
    DialogueEventType::SelfReflection,
    DialogueEventType::Decision,
];

const HUMAN_MULTI_AI_EVENTS: &[DialogueEventType] = &[
    DialogueEventType::Message,
    DialogueEventType::ToolCall,
    DialogueEventType::ToolResult,
    DialogueEventType::Decision,
];

const MULTI_HUMAN_MULTI_AI_EVENTS: &[DialogueEventType] = &[
    DialogueEventType::Message,
    DialogueEventType::Decision,
    DialogueEventType::ToolCall,
    DialogueEventType::ToolResult,
    DialogueEventType::Lifecycle,
];

const AI_GROUP_EVENTS: &[DialogueEventType] = &[
    DialogueEventType::Message,
    DialogueEventType::Decision,
    DialogueEventType::ToolCall,
    DialogueEventType::ToolResult,
];

const AI_SYSTEM_EVENTS: &[DialogueEventType] = &[
    DialogueEventType::Message,
    DialogueEventType::Decision,
    DialogueEventType::System,
];

const SEMANTIC_EDGES_DIALOGUE: &[SemanticEdgeKind] = &[
    SemanticEdgeKind::EventToConcept,
    SemanticEdgeKind::EventToTopic,
    SemanticEdgeKind::EventToEmotion,
    SemanticEdgeKind::ConceptToConcept,
    SemanticEdgeKind::ConceptToTopic,
    SemanticEdgeKind::TopicToTopic,
];

const SCENARIO_RULES: &[ScenarioRule] = &[
    ScenarioRule {
        scenario: ConversationScenario::HumanToHuman,
        description: "双人对话语境，关注情绪与主题扩散",
        primary_event_types: HUMAN_GROUP_EVENTS,
        allowed_semantic_edges: SEMANTIC_EDGES_DIALOGUE,
    },
    ScenarioRule {
        scenario: ConversationScenario::HumanGroup,
        description: "多人类群聊，强调主题和决策演化",
        primary_event_types: HUMAN_GROUP_EVENTS,
        allowed_semantic_edges: SEMANTIC_EDGES_DIALOGUE,
    },
    ScenarioRule {
        scenario: ConversationScenario::HumanToAi,
        description: "人类与单 AI 交互，兼顾工具与意图澄清",
        primary_event_types: HUMAN_TO_AI_EVENTS,
        allowed_semantic_edges: SEMANTIC_EDGES_DIALOGUE,
    },
    ScenarioRule {
        scenario: ConversationScenario::AiToAi,
        description: "AI 与 AI 对话，重点在概念与策略递归",
        primary_event_types: AI_TO_AI_EVENTS,
        allowed_semantic_edges: SEMANTIC_EDGES_DIALOGUE,
    },
    ScenarioRule {
        scenario: ConversationScenario::AiSelfTalk,
        description: "AI 自省场景，突出自反与概念映射",
        primary_event_types: AI_SELF_TALK_EVENTS,
        allowed_semantic_edges: SEMANTIC_EDGES_DIALOGUE,
    },
    ScenarioRule {
        scenario: ConversationScenario::HumanToMultiAi,
        description: "单人类对多 AI，关注协作与工具路由",
        primary_event_types: HUMAN_MULTI_AI_EVENTS,
        allowed_semantic_edges: SEMANTIC_EDGES_DIALOGUE,
    },
    ScenarioRule {
        scenario: ConversationScenario::MultiHumanToMultiAi,
        description: "多人类与多 AI 协作，语义网密度更高",
        primary_event_types: MULTI_HUMAN_MULTI_AI_EVENTS,
        allowed_semantic_edges: SEMANTIC_EDGES_DIALOGUE,
    },
    ScenarioRule {
        scenario: ConversationScenario::AiGroup,
        description: "AI 群组协作，强调知识与概念传递",
        primary_event_types: AI_GROUP_EVENTS,
        allowed_semantic_edges: SEMANTIC_EDGES_DIALOGUE,
    },
    ScenarioRule {
        scenario: ConversationScenario::AiToSystem,
        description: "AI 与系统交互，聚焦系统事件与概念流",
        primary_event_types: AI_SYSTEM_EVENTS,
        allowed_semantic_edges: SEMANTIC_EDGES_DIALOGUE,
    },
];

pub fn scenario_rule(scene: &ConversationScenario) -> &'static ScenarioRule {
    SCENARIO_RULES
        .iter()
        .find(|rule| rule.scenario == *scene)
        .unwrap_or(&SCENARIO_RULES[0])
}
