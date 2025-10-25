use crate::types::SemanticEdgeKind;
use soulseed_agi_core_models::{
    AwarenessFork, ConversationScenario, DialogueEventPayloadKind, DialogueEventType,
};

#[derive(Clone, Debug)]
pub struct ScenarioRule {
    pub scenario: ConversationScenario,
    pub description: &'static str,
    pub primary_event_types: &'static [DialogueEventType],
    pub primary_payloads: &'static [DialogueEventPayloadKind],
    pub allowed_forks: &'static [AwarenessFork],
    pub explain_fields: &'static [&'static str],
    pub allowed_semantic_edges: &'static [SemanticEdgeKind],
}

const HUMAN_TO_AI_EVENTS: &[DialogueEventType] = &[
    DialogueEventType::Message,
    DialogueEventType::ToolCall,
    DialogueEventType::ToolResult,
    DialogueEventType::Decision,
    DialogueEventType::Lifecycle,
];

const HUMAN_TO_AI_PAYLOADS: &[DialogueEventPayloadKind] = &[
    DialogueEventPayloadKind::MessagePrimary,
    DialogueEventPayloadKind::ClarificationIssued,
    DialogueEventPayloadKind::ClarificationAnswered,
    DialogueEventPayloadKind::ToolInvocationDispatched,
    DialogueEventPayloadKind::ToolResultRecorded,
    DialogueEventPayloadKind::AwarenessDecisionRouted,
    DialogueEventPayloadKind::AwarenessSyncPointReported,
];

const HUMAN_TO_AI_FORKS: &[AwarenessFork] = &[
    AwarenessFork::SelfReason,
    AwarenessFork::Clarify,
    AwarenessFork::ToolPath,
    AwarenessFork::Collab,
];

const HUMAN_TO_AI_EXPLAIN: &[&str] = &[
    "decision.explain.router_digest",
    "clarify.questions",
    "tool.plan.nodes",
];

const HUMAN_GROUP_EVENTS: &[DialogueEventType] = &[
    DialogueEventType::Message,
    DialogueEventType::Lifecycle,
    DialogueEventType::System,
];

const HUMAN_GROUP_PAYLOADS: &[DialogueEventPayloadKind] = &[
    DialogueEventPayloadKind::MessagePrimary,
    DialogueEventPayloadKind::MessageBroadcast,
    DialogueEventPayloadKind::LifecycleStageEntered,
    DialogueEventPayloadKind::AwarenessRouteReconsidered,
];

const HUMAN_GROUP_FORKS: &[AwarenessFork] = &[AwarenessFork::Clarify, AwarenessFork::Collab];

const HUMAN_GROUP_EXPLAIN: &[&str] = &["collab.scope", "context.timeline.summary"];

const AI_TO_AI_EVENTS: &[DialogueEventType] = &[
    DialogueEventType::Message,
    DialogueEventType::Decision,
    DialogueEventType::ToolCall,
    DialogueEventType::ToolResult,
];

const AI_TO_AI_PAYLOADS: &[DialogueEventPayloadKind] = &[
    DialogueEventPayloadKind::MessagePrimary,
    DialogueEventPayloadKind::AwarenessDecisionRouted,
    DialogueEventPayloadKind::ToolInvocationDispatched,
    DialogueEventPayloadKind::ToolResultRecorded,
];

const AI_TO_AI_FORKS: &[AwarenessFork] = &[
    AwarenessFork::SelfReason,
    AwarenessFork::ToolPath,
    AwarenessFork::Collab,
];

const AI_TO_AI_EXPLAIN: &[&str] = &["decision.explain.features_snapshot", "tool.plan.barrier"];

const AI_SELF_TALK_EVENTS: &[DialogueEventType] = &[
    DialogueEventType::Message,
    DialogueEventType::SelfReflection,
    DialogueEventType::Decision,
];

const AI_SELF_TALK_PAYLOADS: &[DialogueEventPayloadKind] = &[
    DialogueEventPayloadKind::MessagePrimary,
    DialogueEventPayloadKind::SelfReflectionLogged,
    DialogueEventPayloadKind::AwarenessDecisionRouted,
];

const AI_SELF_TALK_FORKS: &[AwarenessFork] = &[AwarenessFork::SelfReason];

const AI_SELF_TALK_EXPLAIN: &[&str] = &[
    "self_reflection.insight_digest",
    "decision.rationale.blocked",
];

const HUMAN_MULTI_AI_EVENTS: &[DialogueEventType] = &[
    DialogueEventType::Message,
    DialogueEventType::ToolCall,
    DialogueEventType::ToolResult,
    DialogueEventType::Decision,
];

const HUMAN_MULTI_AI_PAYLOADS: &[DialogueEventPayloadKind] = &[
    DialogueEventPayloadKind::MessagePrimary,
    DialogueEventPayloadKind::ToolInvocationDispatched,
    DialogueEventPayloadKind::ToolResultRecorded,
    DialogueEventPayloadKind::AwarenessRouteSwitched,
];

const HUMAN_MULTI_AI_FORKS: &[AwarenessFork] = &[
    AwarenessFork::Clarify,
    AwarenessFork::ToolPath,
    AwarenessFork::Collab,
];

const HUMAN_MULTI_AI_EXPLAIN: &[&str] = &[
    "tool.plan.nodes",
    "collab.scope",
    "decision.explain.thresholds_hit",
];

const MULTI_HUMAN_MULTI_AI_EVENTS: &[DialogueEventType] = &[
    DialogueEventType::Message,
    DialogueEventType::Decision,
    DialogueEventType::ToolCall,
    DialogueEventType::ToolResult,
    DialogueEventType::Lifecycle,
];

const MULTI_HUMAN_MULTI_AI_PAYLOADS: &[DialogueEventPayloadKind] = &[
    DialogueEventPayloadKind::MessagePrimary,
    DialogueEventPayloadKind::ToolInvocationDispatched,
    DialogueEventPayloadKind::ToolResultRecorded,
    DialogueEventPayloadKind::CollabProposed,
    DialogueEventPayloadKind::CollabResolved,
];

const MULTI_HUMAN_MULTI_AI_FORKS: &[AwarenessFork] = &[
    AwarenessFork::Clarify,
    AwarenessFork::ToolPath,
    AwarenessFork::Collab,
];

const MULTI_HUMAN_MULTI_AI_EXPLAIN: &[&str] = &["collab.barrier", "decision.rationale.tradeoff"];

const AI_GROUP_EVENTS: &[DialogueEventType] = &[
    DialogueEventType::Message,
    DialogueEventType::Decision,
    DialogueEventType::ToolCall,
    DialogueEventType::ToolResult,
];

const AI_GROUP_PAYLOADS: &[DialogueEventPayloadKind] = &[
    DialogueEventPayloadKind::MessagePrimary,
    DialogueEventPayloadKind::AwarenessDecisionRouted,
    DialogueEventPayloadKind::ToolInvocationDispatched,
    DialogueEventPayloadKind::ToolResultRecorded,
    DialogueEventPayloadKind::CollabTurnEnded,
];

const AI_GROUP_FORKS: &[AwarenessFork] = &[
    AwarenessFork::SelfReason,
    AwarenessFork::Collab,
    AwarenessFork::ToolPath,
];

const AI_GROUP_EXPLAIN: &[&str] = &["collab.scope", "decision.explain.router_digest"];

const AI_SYSTEM_EVENTS: &[DialogueEventType] = &[
    DialogueEventType::Message,
    DialogueEventType::Decision,
    DialogueEventType::System,
];

const AI_SYSTEM_PAYLOADS: &[DialogueEventPayloadKind] = &[
    DialogueEventPayloadKind::MessagePrimary,
    DialogueEventPayloadKind::AwarenessDecisionRouted,
    DialogueEventPayloadKind::SystemNotificationDispatched,
    DialogueEventPayloadKind::AwarenessContextBuilt,
];

const AI_SYSTEM_FORKS: &[AwarenessFork] = &[AwarenessFork::SelfReason, AwarenessFork::ToolPath];

const AI_SYSTEM_EXPLAIN: &[&str] = &["system.category", "decision.explain.features_snapshot"];

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
        primary_payloads: HUMAN_GROUP_PAYLOADS,
        allowed_forks: HUMAN_GROUP_FORKS,
        explain_fields: HUMAN_GROUP_EXPLAIN,
        allowed_semantic_edges: SEMANTIC_EDGES_DIALOGUE,
    },
    ScenarioRule {
        scenario: ConversationScenario::HumanGroup,
        description: "多人类群聊，强调主题和决策演化",
        primary_event_types: HUMAN_GROUP_EVENTS,
        primary_payloads: HUMAN_GROUP_PAYLOADS,
        allowed_forks: HUMAN_GROUP_FORKS,
        explain_fields: HUMAN_GROUP_EXPLAIN,
        allowed_semantic_edges: SEMANTIC_EDGES_DIALOGUE,
    },
    ScenarioRule {
        scenario: ConversationScenario::HumanToAi,
        description: "人类与单 AI 交互，兼顾工具与意图澄清",
        primary_event_types: HUMAN_TO_AI_EVENTS,
        primary_payloads: HUMAN_TO_AI_PAYLOADS,
        allowed_forks: HUMAN_TO_AI_FORKS,
        explain_fields: HUMAN_TO_AI_EXPLAIN,
        allowed_semantic_edges: SEMANTIC_EDGES_DIALOGUE,
    },
    ScenarioRule {
        scenario: ConversationScenario::AiToAi,
        description: "AI 与 AI 对话，重点在概念与策略递归",
        primary_event_types: AI_TO_AI_EVENTS,
        primary_payloads: AI_TO_AI_PAYLOADS,
        allowed_forks: AI_TO_AI_FORKS,
        explain_fields: AI_TO_AI_EXPLAIN,
        allowed_semantic_edges: SEMANTIC_EDGES_DIALOGUE,
    },
    ScenarioRule {
        scenario: ConversationScenario::AiSelfTalk,
        description: "AI 自省场景，突出自反与概念映射",
        primary_event_types: AI_SELF_TALK_EVENTS,
        primary_payloads: AI_SELF_TALK_PAYLOADS,
        allowed_forks: AI_SELF_TALK_FORKS,
        explain_fields: AI_SELF_TALK_EXPLAIN,
        allowed_semantic_edges: SEMANTIC_EDGES_DIALOGUE,
    },
    ScenarioRule {
        scenario: ConversationScenario::HumanToMultiAi,
        description: "单人类对多 AI，关注协作与工具路由",
        primary_event_types: HUMAN_MULTI_AI_EVENTS,
        primary_payloads: HUMAN_MULTI_AI_PAYLOADS,
        allowed_forks: HUMAN_MULTI_AI_FORKS,
        explain_fields: HUMAN_MULTI_AI_EXPLAIN,
        allowed_semantic_edges: SEMANTIC_EDGES_DIALOGUE,
    },
    ScenarioRule {
        scenario: ConversationScenario::MultiHumanToMultiAi,
        description: "多人类与多 AI 协作，语义网密度更高",
        primary_event_types: MULTI_HUMAN_MULTI_AI_EVENTS,
        primary_payloads: MULTI_HUMAN_MULTI_AI_PAYLOADS,
        allowed_forks: MULTI_HUMAN_MULTI_AI_FORKS,
        explain_fields: MULTI_HUMAN_MULTI_AI_EXPLAIN,
        allowed_semantic_edges: SEMANTIC_EDGES_DIALOGUE,
    },
    ScenarioRule {
        scenario: ConversationScenario::AiGroup,
        description: "AI 群组协作，强调知识与概念传递",
        primary_event_types: AI_GROUP_EVENTS,
        primary_payloads: AI_GROUP_PAYLOADS,
        allowed_forks: AI_GROUP_FORKS,
        explain_fields: AI_GROUP_EXPLAIN,
        allowed_semantic_edges: SEMANTIC_EDGES_DIALOGUE,
    },
    ScenarioRule {
        scenario: ConversationScenario::AiToSystem,
        description: "AI 与系统交互，聚焦系统事件与概念流",
        primary_event_types: AI_SYSTEM_EVENTS,
        primary_payloads: AI_SYSTEM_PAYLOADS,
        allowed_forks: AI_SYSTEM_FORKS,
        explain_fields: AI_SYSTEM_EXPLAIN,
        allowed_semantic_edges: SEMANTIC_EDGES_DIALOGUE,
    },
];

pub fn scenario_rule(scene: &ConversationScenario) -> &'static ScenarioRule {
    SCENARIO_RULES
        .iter()
        .find(|rule| rule.scenario == *scene)
        .unwrap_or(&SCENARIO_RULES[0])
}
