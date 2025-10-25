use crate::{
    awareness::{
        AwarenessDegradationReason, AwarenessEventType, DecisionPath, DeltaPatch, SyncPointReport,
        ToolPlan,
    },
    common::{EvidencePointer, ModelError, SubjectRef},
    enums::DialogueEventType,
    ids::EventId,
    legacy::dialogue_event::{MessagePointer, SelfReflectionRecord, ToolInvocation, ToolResult},
};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum DialoguePayloadDomain {
    Message,
    Clarification,
    Tooling,
    Collaboration,
    Hitl,
    SyncPoint,
    Awareness,
    Lifecycle,
    System,
    Environment,
    SelfReflection,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MessagePayload {
    pub message: MessagePointer,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_ref: Option<ContentReference>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reply_to: Option<MessagePointer>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub mentions: Vec<SubjectRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub channel: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sentiment: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub is_final: Option<bool>,
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub attributes: Value,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct ClarificationPayload {
    pub clarification_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub question_ref: Option<ContentReference>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub answer_ref: Option<ContentReference>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub assigned_to: Option<SubjectRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ttl_ms: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub merged_into: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub degradation_reason: Option<AwarenessDegradationReason>,
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub attributes: Value,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct ToolPayload {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plan: Option<ToolPlan>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub invocation: Option<ToolInvocation>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result: Option<ToolResult>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub barrier_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub route_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attempt: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub latency_ms: Option<u32>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub evidence: Vec<EvidencePointer>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_digest_sha256: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub blob_ref: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub degradation_reason: Option<AwarenessDegradationReason>,
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub attributes: Value,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct CollaborationPayload {
    pub scope_id: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub participants: Vec<SubjectRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub barrier_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary_ref: Option<ContentReference>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub assigned_to: Option<SubjectRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub degradation_reason: Option<AwarenessDegradationReason>,
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub attributes: Value,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct HitlPayload {
    pub injection_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub priority: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub queue_position: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub actor: Option<SubjectRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub instructions_ref: Option<ContentReference>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resolution_ref: Option<ContentReference>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub degradation_reason: Option<AwarenessDegradationReason>,
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub attributes: Value,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct SyncPointPayload {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sync_point_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub report: Option<SyncPointReport>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub window_start_ms: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub window_end_ms: Option<i64>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub absorbed_signals: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub degradation_reason: Option<AwarenessDegradationReason>,
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub attributes: Value,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AwarenessPayload {
    pub event_type: AwarenessEventType,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub decision_path: Option<DecisionPath>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub delta_patch: Option<DeltaPatch>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sync_point_report: Option<SyncPointReport>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning_trace: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning_confidence: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning_strategy: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub degradation_reason: Option<AwarenessDegradationReason>,
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub attributes: Value,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct LifecyclePayload {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stage: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub supersedes: Option<EventId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub superseded_by: Option<EventId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_event_id: Option<EventId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub chain_length: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub checkpoint_id: Option<String>,
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub attributes: Value,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct SystemPayload {
    pub category: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub related_service: Option<String>,
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub detail: Value,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct EnvironmentPayload {
    pub snapshot_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub diff_digest: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metrics_digest: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub anomaly_score: Option<f32>,
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub attributes: Value,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SelfReflectionPayload {
    pub record: SelfReflectionRecord,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub insight_digest: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub related_event_id: Option<EventId>,
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub attributes: Value,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct ContentReference {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<MessagePointer>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub digest_sha256: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub evidence: Option<EvidencePointer>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub blob_ref: Option<String>,
}

impl ContentReference {
    pub fn is_empty(&self) -> bool {
        self.message.is_none()
            && self.digest_sha256.is_none()
            && self.evidence.is_none()
            && self.blob_ref.is_none()
    }
}

pub trait DialoguePayloadData {
    fn validate_with_kind(&self, kind: DialogueEventPayloadKind) -> Result<(), ModelError>;
}

macro_rules! define_payload_enum {
    ($(
        $(#[$meta:meta])* $variant:ident => { name: $name:literal, domain: $domain:ident, data: $data:ty }
    ),+ $(,)?) => {
        #[derive(Clone, Debug, Serialize, Deserialize)]
        #[serde(tag = "kind", content = "data")]
        pub enum DialogueEventPayload {
            $(
                $(#[$meta])*
                #[serde(rename = $name)]
                $variant($data)
            ),+
        }

        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
        #[serde(rename_all = "snake_case")]
        pub enum DialogueEventPayloadKind {
            $( $variant ),+
        }

        impl DialogueEventPayloadKind {
            pub const fn as_str(&self) -> &'static str {
                match self {
                    $( Self::$variant => $name ),+
                }
            }

            pub const fn domain(&self) -> DialoguePayloadDomain {
                match self {
                    $( Self::$variant => DialoguePayloadDomain::$domain ),+
                }
            }
        }

        impl std::fmt::Display for DialogueEventPayloadKind {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str(self.as_str())
            }
        }

        impl DialogueEventPayload {
            pub const fn kind(&self) -> DialogueEventPayloadKind {
                match self {
                    $( Self::$variant(_) => DialogueEventPayloadKind::$variant ),+
                }
            }

            pub const fn domain(&self) -> DialoguePayloadDomain {
                self.kind().domain()
            }

            fn validate_data(&self) -> Result<(), ModelError> {
                match self {
                    $( Self::$variant(inner) => inner.validate_with_kind(DialogueEventPayloadKind::$variant) ),+
                }
            }
        }
    };
}

define_payload_enum! {
    // Message domain
    MessagePrimary => { name: "message_primary", domain: Message, data: MessagePayload },
    MessageEdited => { name: "message_edited", domain: Message, data: MessagePayload },
    MessageRegenerated => { name: "message_regenerated", domain: Message, data: MessagePayload },
    MessageSummaryPublished => { name: "message_summary_published", domain: Message, data: MessagePayload },
    MessageRecalled => { name: "message_recalled", domain: Message, data: MessagePayload },
    MessagePinned => { name: "message_pinned", domain: Message, data: MessagePayload },
    MessageFeedbackCaptured => { name: "message_feedback_captured", domain: Message, data: MessagePayload },
    MessageReactionAdded => { name: "message_reaction_added", domain: Message, data: MessagePayload },
    MessageReactionRemoved => { name: "message_reaction_removed", domain: Message, data: MessagePayload },
    MessageThreadStarted => { name: "message_thread_started", domain: Message, data: MessagePayload },
    MessageThreadReplied => { name: "message_thread_replied", domain: Message, data: MessagePayload },
    MessageAttachmentLinked => { name: "message_attachment_linked", domain: Message, data: MessagePayload },
    MessageAttachmentRemoved => { name: "message_attachment_removed", domain: Message, data: MessagePayload },
    MessageTranslated => { name: "message_translated", domain: Message, data: MessagePayload },
    MessageBroadcast => { name: "message_broadcast", domain: Message, data: MessagePayload },

    // Clarification domain
    ClarificationIssued => { name: "clarification_issued", domain: Clarification, data: ClarificationPayload },
    ClarificationReminderSent => { name: "clarification_reminder_sent", domain: Clarification, data: ClarificationPayload },
    ClarificationEscalated => { name: "clarification_escalated", domain: Clarification, data: ClarificationPayload },
    ClarificationAnswered => { name: "clarification_answered", domain: Clarification, data: ClarificationPayload },
    ClarificationAutoResolved => { name: "clarification_auto_resolved", domain: Clarification, data: ClarificationPayload },
    ClarificationExpired => { name: "clarification_expired", domain: Clarification, data: ClarificationPayload },
    ClarificationDeferred => { name: "clarification_deferred", domain: Clarification, data: ClarificationPayload },
    ClarificationDelegated => { name: "clarification_delegated", domain: Clarification, data: ClarificationPayload },
    ClarificationMerged => { name: "clarification_merged", domain: Clarification, data: ClarificationPayload },
    ClarificationSuppressed => { name: "clarification_suppressed", domain: Clarification, data: ClarificationPayload },
    ClarificationAborted => { name: "clarification_aborted", domain: Clarification, data: ClarificationPayload },
    ClarificationAcknowledged => { name: "clarification_acknowledged", domain: Clarification, data: ClarificationPayload },

    // Tooling domain
    ToolPlanDrafted => { name: "tool_plan_drafted", domain: Tooling, data: ToolPayload },
    ToolPlanValidated => { name: "tool_plan_validated", domain: Tooling, data: ToolPayload },
    ToolPlanRejected => { name: "tool_plan_rejected", domain: Tooling, data: ToolPayload },
    ToolPlanCommitted => { name: "tool_plan_committed", domain: Tooling, data: ToolPayload },
    ToolPathDecided => { name: "tool_path_decided", domain: Tooling, data: ToolPayload },
    ToolInvocationScheduled => { name: "tool_invocation_scheduled", domain: Tooling, data: ToolPayload },
    ToolInvocationDispatched => { name: "tool_invocation_dispatched", domain: Tooling, data: ToolPayload },
    ToolInvocationStarted => { name: "tool_invocation_started", domain: Tooling, data: ToolPayload },
    ToolInvocationProgressed => { name: "tool_invocation_progressed", domain: Tooling, data: ToolPayload },
    ToolInvocationCompleted => { name: "tool_invocation_completed", domain: Tooling, data: ToolPayload },
    ToolInvocationFailed => { name: "tool_invocation_failed", domain: Tooling, data: ToolPayload },
    ToolInvocationRetried => { name: "tool_invocation_retried", domain: Tooling, data: ToolPayload },
    ToolResultRecorded => { name: "tool_result_recorded", domain: Tooling, data: ToolPayload },
    ToolResultAppended => { name: "tool_result_appended", domain: Tooling, data: ToolPayload },
    ToolBarrierReached => { name: "tool_barrier_reached", domain: Tooling, data: ToolPayload },
    ToolBarrierReleased => { name: "tool_barrier_released", domain: Tooling, data: ToolPayload },
    ToolBarrierTimeout => { name: "tool_barrier_timeout", domain: Tooling, data: ToolPayload },
    ToolRouteSwitched => { name: "tool_route_switched", domain: Tooling, data: ToolPayload },

    // Collaboration domain
    CollabProposed => { name: "collab_proposed", domain: Collaboration, data: CollaborationPayload },
    CollabScopeNegotiated => { name: "collab_scope_negotiated", domain: Collaboration, data: CollaborationPayload },
    CollabRequested => { name: "collab_requested", domain: Collaboration, data: CollaborationPayload },
    CollabAccepted => { name: "collab_accepted", domain: Collaboration, data: CollaborationPayload },
    CollabDeclined => { name: "collab_declined", domain: Collaboration, data: CollaborationPayload },
    CollabInProgress => { name: "collab_in_progress", domain: Collaboration, data: CollaborationPayload },
    CollabTurnEnded => { name: "collab_turn_ended", domain: Collaboration, data: CollaborationPayload },
    CollabBarrierReached => { name: "collab_barrier_reached", domain: Collaboration, data: CollaborationPayload },
    CollabMerged => { name: "collab_merged", domain: Collaboration, data: CollaborationPayload },
    CollabResolved => { name: "collab_resolved", domain: Collaboration, data: CollaborationPayload },
    CollabAborted => { name: "collab_aborted", domain: Collaboration, data: CollaborationPayload },
    CollabSummaryPublished => { name: "collab_summary_published", domain: Collaboration, data: CollaborationPayload },
    CollabParticipantJoined => { name: "collab_participant_joined", domain: Collaboration, data: CollaborationPayload },
    CollabParticipantLeft => { name: "collab_participant_left", domain: Collaboration, data: CollaborationPayload },

    // HITL domain
    HitlInjectionReceived => { name: "hitl_injection_received", domain: Hitl, data: HitlPayload },
    HitlInjectionPrioritized => { name: "hitl_injection_prioritized", domain: Hitl, data: HitlPayload },
    HitlInjectionAssigned => { name: "hitl_injection_assigned", domain: Hitl, data: HitlPayload },
    HitlInjectionAcknowledged => { name: "hitl_injection_acknowledged", domain: Hitl, data: HitlPayload },
    HitlInjectionApplied => { name: "hitl_injection_applied", domain: Hitl, data: HitlPayload },
    HitlInjectionDeferred => { name: "hitl_injection_deferred", domain: Hitl, data: HitlPayload },
    HitlInjectionIgnored => { name: "hitl_injection_ignored", domain: Hitl, data: HitlPayload },
    HitlAbortRequested => { name: "hitl_abort_requested", domain: Hitl, data: HitlPayload },
    HitlAbortApproved => { name: "hitl_abort_approved", domain: Hitl, data: HitlPayload },
    HitlAbortRejected => { name: "hitl_abort_rejected", domain: Hitl, data: HitlPayload },
    HitlFeedbackRecorded => { name: "hitl_feedback_recorded", domain: Hitl, data: HitlPayload },
    HitlQueueReordered => { name: "hitl_queue_reordered", domain: Hitl, data: HitlPayload },
    HitlResolutionPublished => { name: "hitl_resolution_published", domain: Hitl, data: HitlPayload },

    // Sync point domain
    SyncPointMerged => { name: "sync_point_merged", domain: SyncPoint, data: SyncPointPayload },
    SyncPointReportPublished => { name: "sync_point_report_published", domain: SyncPoint, data: SyncPointPayload },
    SyncPointBarrierRaised => { name: "sync_point_barrier_raised", domain: SyncPoint, data: SyncPointPayload },
    SyncPointBarrierCleared => { name: "sync_point_barrier_cleared", domain: SyncPoint, data: SyncPointPayload },
    SyncPointClarifyWindowOpened => { name: "sync_point_clarify_window_opened", domain: SyncPoint, data: SyncPointPayload },
    SyncPointClarifyWindowClosed => { name: "sync_point_clarify_window_closed", domain: SyncPoint, data: SyncPointPayload },
    SyncPointToolWindowOpened => { name: "sync_point_tool_window_opened", domain: SyncPoint, data: SyncPointPayload },
    SyncPointToolWindowClosed => { name: "sync_point_tool_window_closed", domain: SyncPoint, data: SyncPointPayload },
    SyncPointHitlWindowOpened => { name: "sync_point_hitl_window_opened", domain: SyncPoint, data: SyncPointPayload },
    SyncPointHitlWindowClosed => { name: "sync_point_hitl_window_closed", domain: SyncPoint, data: SyncPointPayload },
    SyncPointDriftDetected => { name: "sync_point_drift_detected", domain: SyncPoint, data: SyncPointPayload },
    SyncPointLateSignalObserved => { name: "sync_point_late_signal_observed", domain: SyncPoint, data: SyncPointPayload },
    SyncPointBudgetExceeded => { name: "sync_point_budget_exceeded", domain: SyncPoint, data: SyncPointPayload },
    SyncPointBudgetRecovered => { name: "sync_point_budget_recovered", domain: SyncPoint, data: SyncPointPayload },
    SyncPointDegradationRecorded => { name: "sync_point_degradation_recorded", domain: SyncPoint, data: SyncPointPayload },

    // Awareness domain
    AwarenessAcStarted => { name: "awareness_ac_started", domain: Awareness, data: AwarenessPayload },
    AwarenessIcStarted => { name: "awareness_ic_started", domain: Awareness, data: AwarenessPayload },
    AwarenessIcEnded => { name: "awareness_ic_ended", domain: Awareness, data: AwarenessPayload },
    AwarenessAssessmentProduced => { name: "awareness_assessment_produced", domain: Awareness, data: AwarenessPayload },
    AwarenessDecisionRouted => { name: "awareness_decision_routed", domain: Awareness, data: AwarenessPayload },
    AwarenessRouteReconsidered => { name: "awareness_route_reconsidered", domain: Awareness, data: AwarenessPayload },
    AwarenessRouteSwitched => { name: "awareness_route_switched", domain: Awareness, data: AwarenessPayload },
    AwarenessDeltaPatchGenerated => { name: "awareness_delta_patch_generated", domain: Awareness, data: AwarenessPayload },
    AwarenessContextBuilt => { name: "awareness_context_built", domain: Awareness, data: AwarenessPayload },
    AwarenessDeltaMerged => { name: "awareness_delta_merged", domain: Awareness, data: AwarenessPayload },
    AwarenessFinalized => { name: "awareness_finalized", domain: Awareness, data: AwarenessPayload },
    AwarenessRejected => { name: "awareness_rejected", domain: Awareness, data: AwarenessPayload },
    AwarenessSyncPointReported => { name: "awareness_sync_point_reported", domain: Awareness, data: AwarenessPayload },
    AwarenessLateReceiptObserved => { name: "awareness_late_receipt_observed", domain: Awareness, data: AwarenessPayload },

    // Lifecycle domain
    LifecycleStageEntered => { name: "lifecycle_stage_entered", domain: Lifecycle, data: LifecyclePayload },
    LifecycleStageExited => { name: "lifecycle_stage_exited", domain: Lifecycle, data: LifecyclePayload },
    LifecycleCheckpointCreated => { name: "lifecycle_checkpoint_created", domain: Lifecycle, data: LifecyclePayload },
    LifecycleCheckpointRestored => { name: "lifecycle_checkpoint_restored", domain: Lifecycle, data: LifecyclePayload },
    LifecycleSuperseded => { name: "lifecycle_superseded", domain: Lifecycle, data: LifecyclePayload },
    LifecycleSupersessionChainUpdated => { name: "lifecycle_supersession_chain_updated", domain: Lifecycle, data: LifecyclePayload },
    LifecycleParentLinked => { name: "lifecycle_parent_linked", domain: Lifecycle, data: LifecyclePayload },
    LifecycleParentUnlinked => { name: "lifecycle_parent_unlinked", domain: Lifecycle, data: LifecyclePayload },
    LifecycleVersionPinned => { name: "lifecycle_version_pinned", domain: Lifecycle, data: LifecyclePayload },
    LifecycleVersionReleased => { name: "lifecycle_version_released", domain: Lifecycle, data: LifecyclePayload },

    // System domain
    SystemNotificationDispatched => { name: "system_notification_dispatched", domain: System, data: SystemPayload },
    SystemPolicyApplied => { name: "system_policy_applied", domain: System, data: SystemPayload },
    SystemPolicyViolated => { name: "system_policy_violated", domain: System, data: SystemPayload },
    SystemRateLimited => { name: "system_rate_limited", domain: System, data: SystemPayload },
    SystemDegradationDetected => { name: "system_degradation_detected", domain: System, data: SystemPayload },
    SystemDegradationRecovered => { name: "system_degradation_recovered", domain: System, data: SystemPayload },
    SystemAuditLogged => { name: "system_audit_logged", domain: System, data: SystemPayload },
    SystemMaintenanceScheduled => { name: "system_maintenance_scheduled", domain: System, data: SystemPayload },

    // Environment domain
    EnvironmentSnapshotRecorded => { name: "environment_snapshot_recorded", domain: Environment, data: EnvironmentPayload },
    EnvironmentSnapshotDiffed => { name: "environment_snapshot_diffed", domain: Environment, data: EnvironmentPayload },
    EnvironmentSignalIngested => { name: "environment_signal_ingested", domain: Environment, data: EnvironmentPayload },
    EnvironmentSignalDropped => { name: "environment_signal_dropped", domain: Environment, data: EnvironmentPayload },
    EnvironmentContextExpanded => { name: "environment_context_expanded", domain: Environment, data: EnvironmentPayload },
    EnvironmentContextPruned => { name: "environment_context_pruned", domain: Environment, data: EnvironmentPayload },
    EnvironmentMetricAnomaly => { name: "environment_metric_anomaly", domain: Environment, data: EnvironmentPayload },
    EnvironmentChannelUpdated => { name: "environment_channel_updated", domain: Environment, data: EnvironmentPayload },

    // Self-reflection domain
    SelfReflectionLogged => { name: "self_reflection_logged", domain: SelfReflection, data: SelfReflectionPayload },
    SelfReflectionHypothesisFormed => { name: "self_reflection_hypothesis_formed", domain: SelfReflection, data: SelfReflectionPayload },
    SelfReflectionActionCommitted => { name: "self_reflection_action_committed", domain: SelfReflection, data: SelfReflectionPayload },
    SelfReflectionScoreAdjusted => { name: "self_reflection_score_adjusted", domain: SelfReflection, data: SelfReflectionPayload },
    SelfReflectionArchived => { name: "self_reflection_archived", domain: SelfReflection, data: SelfReflectionPayload },
}

impl DialogueEventPayload {
    pub fn validate_for_event_type(&self, event_type: DialogueEventType) -> Result<(), ModelError> {
        ensure_type_alignment(event_type, self.domain())?;
        self.validate_data()
    }
}

fn ensure_type_alignment(
    event_type: DialogueEventType,
    domain: DialoguePayloadDomain,
) -> Result<(), ModelError> {
    let ok = match domain {
        DialoguePayloadDomain::Message => matches!(event_type, DialogueEventType::Message),
        DialoguePayloadDomain::Clarification => matches!(
            event_type,
            DialogueEventType::Decision | DialogueEventType::Lifecycle
        ),
        DialoguePayloadDomain::Tooling => matches!(
            event_type,
            DialogueEventType::ToolCall | DialogueEventType::ToolResult
        ),
        DialoguePayloadDomain::Collaboration => matches!(
            event_type,
            DialogueEventType::Decision | DialogueEventType::Lifecycle
        ),
        DialoguePayloadDomain::Hitl => matches!(
            event_type,
            DialogueEventType::Lifecycle | DialogueEventType::System
        ),
        DialoguePayloadDomain::SyncPoint => matches!(
            event_type,
            DialogueEventType::Lifecycle | DialogueEventType::System
        ),
        DialoguePayloadDomain::Awareness => matches!(
            event_type,
            DialogueEventType::Decision | DialogueEventType::Lifecycle | DialogueEventType::System
        ),
        DialoguePayloadDomain::Lifecycle => matches!(event_type, DialogueEventType::Lifecycle),
        DialoguePayloadDomain::System => matches!(event_type, DialogueEventType::System),
        DialoguePayloadDomain::Environment => matches!(event_type, DialogueEventType::System),
        DialoguePayloadDomain::SelfReflection => {
            matches!(event_type, DialogueEventType::SelfReflection)
        }
    };

    if ok {
        Ok(())
    } else {
        Err(ModelError::Invariant(
            "payload kind incompatible with event_type",
        ))
    }
}

fn ensure_not_blank(value: &str, field: &'static str) -> Result<(), ModelError> {
    if value.trim().is_empty() {
        Err(ModelError::Invariant(field))
    } else {
        Ok(())
    }
}

fn ensure_option_not_blank(value: &Option<String>, field: &'static str) -> Result<(), ModelError> {
    if let Some(v) = value {
        ensure_not_blank(v, field)?;
    }
    Ok(())
}

impl DialoguePayloadData for MessagePayload {
    fn validate_with_kind(&self, kind: DialogueEventPayloadKind) -> Result<(), ModelError> {
        if let Some(channel) = &self.channel {
            ensure_not_blank(channel, "payload.channel")?;
        }
        if let Some(language) = &self.language {
            ensure_not_blank(language, "payload.language")?;
        }
        if let Some(sentiment) = &self.sentiment {
            ensure_not_blank(sentiment, "payload.sentiment")?;
        }
        match kind {
            DialogueEventPayloadKind::MessageSummaryPublished
            | DialogueEventPayloadKind::MessageAttachmentLinked
            | DialogueEventPayloadKind::MessageAttachmentRemoved
            | DialogueEventPayloadKind::MessageTranslated
            | DialogueEventPayloadKind::MessageBroadcast => {
                if self.content_ref.is_none() {
                    return Err(ModelError::Missing("payload.content_ref"));
                }
            }
            DialogueEventPayloadKind::MessageThreadReplied => {
                if self.reply_to.is_none() {
                    return Err(ModelError::Missing("payload.reply_to"));
                }
            }
            DialogueEventPayloadKind::MessageFeedbackCaptured
            | DialogueEventPayloadKind::MessageReactionAdded
            | DialogueEventPayloadKind::MessageReactionRemoved => {
                if self.attributes.is_null() {
                    return Err(ModelError::Missing("payload.attributes"));
                }
            }
            _ => {}
        }
        Ok(())
    }
}

impl DialoguePayloadData for ClarificationPayload {
    fn validate_with_kind(&self, kind: DialogueEventPayloadKind) -> Result<(), ModelError> {
        ensure_not_blank(&self.clarification_id, "payload.clarification_id")?;
        if let Some(ttl) = self.ttl_ms {
            if ttl == 0 {
                return Err(ModelError::Invariant("payload.ttl_ms"));
            }
        }
        match kind {
            DialogueEventPayloadKind::ClarificationIssued
            | DialogueEventPayloadKind::ClarificationReminderSent
            | DialogueEventPayloadKind::ClarificationEscalated => {
                if self.question_ref.is_none() {
                    return Err(ModelError::Missing("payload.question_ref"));
                }
            }
            DialogueEventPayloadKind::ClarificationAnswered
            | DialogueEventPayloadKind::ClarificationAutoResolved
            | DialogueEventPayloadKind::ClarificationAcknowledged => {
                if self.answer_ref.is_none() {
                    return Err(ModelError::Missing("payload.answer_ref"));
                }
            }
            DialogueEventPayloadKind::ClarificationDelegated => {
                if self.assigned_to.is_none() {
                    return Err(ModelError::Missing("payload.assigned_to"));
                }
            }
            DialogueEventPayloadKind::ClarificationMerged => {
                ensure_option_not_blank(&self.merged_into, "payload.merged_into")?;
            }
            DialogueEventPayloadKind::ClarificationSuppressed => {
                ensure_option_not_blank(&self.parent_id, "payload.parent_id")?;
            }
            _ => {}
        }
        Ok(())
    }
}

impl DialoguePayloadData for ToolPayload {
    fn validate_with_kind(&self, kind: DialogueEventPayloadKind) -> Result<(), ModelError> {
        if let Some(route_id) = &self.route_id {
            ensure_not_blank(route_id, "payload.route_id")?;
        }
        if let Some(barrier_id) = &self.barrier_id {
            ensure_not_blank(barrier_id, "payload.barrier_id")?;
        }
        match kind {
            DialogueEventPayloadKind::ToolPlanDrafted
            | DialogueEventPayloadKind::ToolPlanValidated
            | DialogueEventPayloadKind::ToolPlanRejected
            | DialogueEventPayloadKind::ToolPlanCommitted
            | DialogueEventPayloadKind::ToolPathDecided => {
                if self.plan.is_none() {
                    return Err(ModelError::Missing("payload.plan"));
                }
            }
            DialogueEventPayloadKind::ToolInvocationScheduled
            | DialogueEventPayloadKind::ToolInvocationDispatched
            | DialogueEventPayloadKind::ToolInvocationStarted
            | DialogueEventPayloadKind::ToolInvocationProgressed => {
                if self.invocation.is_none() {
                    return Err(ModelError::Missing("payload.invocation"));
                }
            }
            DialogueEventPayloadKind::ToolInvocationCompleted
            | DialogueEventPayloadKind::ToolInvocationFailed
            | DialogueEventPayloadKind::ToolResultRecorded
            | DialogueEventPayloadKind::ToolResultAppended => {
                if self.result.is_none() {
                    return Err(ModelError::Missing("payload.result"));
                }
            }
            DialogueEventPayloadKind::ToolInvocationRetried => {
                if self.invocation.is_none() {
                    return Err(ModelError::Missing("payload.invocation"));
                }
                if self.attempt.is_none() {
                    return Err(ModelError::Missing("payload.attempt"));
                }
            }
            DialogueEventPayloadKind::ToolBarrierReached
            | DialogueEventPayloadKind::ToolBarrierReleased
            | DialogueEventPayloadKind::ToolBarrierTimeout => {
                if self.barrier_id.is_none() {
                    return Err(ModelError::Missing("payload.barrier_id"));
                }
            }
            DialogueEventPayloadKind::ToolRouteSwitched => {
                if self.route_id.is_none() {
                    return Err(ModelError::Missing("payload.route_id"));
                }
            }
            _ => {}
        }
        Ok(())
    }
}

impl DialoguePayloadData for CollaborationPayload {
    fn validate_with_kind(&self, kind: DialogueEventPayloadKind) -> Result<(), ModelError> {
        ensure_not_blank(&self.scope_id, "payload.scope_id")?;
        match kind {
            DialogueEventPayloadKind::CollabParticipantJoined
            | DialogueEventPayloadKind::CollabParticipantLeft
            | DialogueEventPayloadKind::CollabProposed
            | DialogueEventPayloadKind::CollabRequested => {
                if self.participants.is_empty() {
                    return Err(ModelError::Missing("payload.participants"));
                }
            }
            DialogueEventPayloadKind::CollabBarrierReached => {
                if self.barrier_id.is_none() {
                    return Err(ModelError::Missing("payload.barrier_id"));
                }
            }
            DialogueEventPayloadKind::CollabSummaryPublished => {
                if self.summary_ref.is_none() {
                    return Err(ModelError::Missing("payload.summary_ref"));
                }
            }
            DialogueEventPayloadKind::CollabScopeNegotiated => {
                if self.attributes.is_null() {
                    return Err(ModelError::Missing("payload.attributes"));
                }
            }
            _ => {}
        }
        Ok(())
    }
}

impl DialoguePayloadData for HitlPayload {
    fn validate_with_kind(&self, kind: DialogueEventPayloadKind) -> Result<(), ModelError> {
        ensure_not_blank(&self.injection_id, "payload.injection_id")?;
        match kind {
            DialogueEventPayloadKind::HitlInjectionAssigned
            | DialogueEventPayloadKind::HitlInjectionAcknowledged
            | DialogueEventPayloadKind::HitlAbortRequested
            | DialogueEventPayloadKind::HitlAbortApproved
            | DialogueEventPayloadKind::HitlAbortRejected => {
                if self.actor.is_none() {
                    return Err(ModelError::Missing("payload.actor"));
                }
            }
            DialogueEventPayloadKind::HitlInjectionApplied
            | DialogueEventPayloadKind::HitlResolutionPublished => {
                if self.resolution_ref.is_none() {
                    return Err(ModelError::Missing("payload.resolution_ref"));
                }
            }
            DialogueEventPayloadKind::HitlQueueReordered => {
                if self.queue_position.is_none() {
                    return Err(ModelError::Missing("payload.queue_position"));
                }
            }
            _ => {}
        }
        Ok(())
    }
}

impl DialoguePayloadData for SyncPointPayload {
    fn validate_with_kind(&self, kind: DialogueEventPayloadKind) -> Result<(), ModelError> {
        match kind {
            DialogueEventPayloadKind::SyncPointReportPublished => {
                if self.report.is_none() {
                    return Err(ModelError::Missing("payload.report"));
                }
            }
            DialogueEventPayloadKind::SyncPointBarrierRaised
            | DialogueEventPayloadKind::SyncPointBarrierCleared
            | DialogueEventPayloadKind::SyncPointClarifyWindowOpened
            | DialogueEventPayloadKind::SyncPointClarifyWindowClosed
            | DialogueEventPayloadKind::SyncPointToolWindowOpened
            | DialogueEventPayloadKind::SyncPointToolWindowClosed
            | DialogueEventPayloadKind::SyncPointHitlWindowOpened
            | DialogueEventPayloadKind::SyncPointHitlWindowClosed
            | DialogueEventPayloadKind::SyncPointMerged => {
                ensure_option_not_blank(&self.sync_point_id, "payload.sync_point_id")?;
            }
            DialogueEventPayloadKind::SyncPointBudgetExceeded
            | DialogueEventPayloadKind::SyncPointDegradationRecorded => {
                if self.degradation_reason.is_none() {
                    return Err(ModelError::Missing("payload.degradation_reason"));
                }
            }
            _ => {}
        }
        Ok(())
    }
}

impl DialoguePayloadData for AwarenessPayload {
    fn validate_with_kind(&self, kind: DialogueEventPayloadKind) -> Result<(), ModelError> {
        if let Some(conf) = self.reasoning_confidence {
            if !(0.0..=1.0).contains(&conf) {
                return Err(ModelError::Invariant("payload.reasoning_confidence"));
            }
        }

        let expected = match kind {
            DialogueEventPayloadKind::AwarenessAcStarted => AwarenessEventType::AcStarted,
            DialogueEventPayloadKind::AwarenessIcStarted => AwarenessEventType::IcStarted,
            DialogueEventPayloadKind::AwarenessIcEnded => AwarenessEventType::IcEnded,
            DialogueEventPayloadKind::AwarenessAssessmentProduced => {
                AwarenessEventType::AssessmentProduced
            }
            DialogueEventPayloadKind::AwarenessDecisionRouted => AwarenessEventType::DecisionRouted,
            DialogueEventPayloadKind::AwarenessRouteReconsidered => {
                AwarenessEventType::RouteReconsidered
            }
            DialogueEventPayloadKind::AwarenessRouteSwitched => AwarenessEventType::RouteSwitched,
            DialogueEventPayloadKind::AwarenessDeltaPatchGenerated => {
                AwarenessEventType::DeltaPatchGenerated
            }
            DialogueEventPayloadKind::AwarenessContextBuilt => AwarenessEventType::ContextBuilt,
            DialogueEventPayloadKind::AwarenessDeltaMerged => AwarenessEventType::DeltaMerged,
            DialogueEventPayloadKind::AwarenessFinalized => AwarenessEventType::Finalized,
            DialogueEventPayloadKind::AwarenessRejected => AwarenessEventType::Rejected,
            DialogueEventPayloadKind::AwarenessSyncPointReported => {
                AwarenessEventType::SyncPointReported
            }
            DialogueEventPayloadKind::AwarenessLateReceiptObserved => {
                AwarenessEventType::LateReceiptObserved
            }
            _ => return Err(ModelError::Invariant("payload.awareness_event_type")),
        };

        if self.event_type != expected {
            return Err(ModelError::Invariant("payload.awareness_event_type"));
        }

        match kind {
            DialogueEventPayloadKind::AwarenessDeltaPatchGenerated => {
                if self.delta_patch.is_none() {
                    return Err(ModelError::Missing("payload.delta_patch"));
                }
            }
            DialogueEventPayloadKind::AwarenessSyncPointReported => {
                if self.sync_point_report.is_none() {
                    return Err(ModelError::Missing("payload.sync_point_report"));
                }
            }
            _ => {}
        }

        Ok(())
    }
}

impl DialoguePayloadData for LifecyclePayload {
    fn validate_with_kind(&self, kind: DialogueEventPayloadKind) -> Result<(), ModelError> {
        match kind {
            DialogueEventPayloadKind::LifecycleStageEntered
            | DialogueEventPayloadKind::LifecycleStageExited
            | DialogueEventPayloadKind::LifecycleVersionPinned
            | DialogueEventPayloadKind::LifecycleVersionReleased => {
                if let Some(stage) = &self.stage {
                    ensure_not_blank(stage, "payload.stage")?;
                } else {
                    return Err(ModelError::Missing("payload.stage"));
                }
            }
            DialogueEventPayloadKind::LifecycleCheckpointCreated
            | DialogueEventPayloadKind::LifecycleCheckpointRestored => {
                ensure_option_not_blank(&self.checkpoint_id, "payload.checkpoint_id")?;
            }
            DialogueEventPayloadKind::LifecycleSuperseded => {
                if self.supersedes.is_none() {
                    return Err(ModelError::Missing("payload.supersedes"));
                }
            }
            DialogueEventPayloadKind::LifecycleSupersessionChainUpdated => {
                if let Some(len) = self.chain_length {
                    if len == 0 {
                        return Err(ModelError::Invariant("payload.chain_length"));
                    }
                } else {
                    return Err(ModelError::Missing("payload.chain_length"));
                }
            }
            DialogueEventPayloadKind::LifecycleParentLinked
            | DialogueEventPayloadKind::LifecycleParentUnlinked => {
                if self.parent_event_id.is_none() {
                    return Err(ModelError::Missing("payload.parent_event_id"));
                }
            }
            _ => {}
        }
        Ok(())
    }
}

impl DialoguePayloadData for SystemPayload {
    fn validate_with_kind(&self, _kind: DialogueEventPayloadKind) -> Result<(), ModelError> {
        ensure_not_blank(&self.category, "payload.category")?;
        Ok(())
    }
}

impl DialoguePayloadData for EnvironmentPayload {
    fn validate_with_kind(&self, kind: DialogueEventPayloadKind) -> Result<(), ModelError> {
        ensure_not_blank(&self.snapshot_id, "payload.snapshot_id")?;
        if let Some(diff) = &self.diff_digest {
            ensure_not_blank(diff, "payload.diff_digest")?;
        }
        if let Some(metrics) = &self.metrics_digest {
            ensure_not_blank(metrics, "payload.metrics_digest")?;
        }
        match kind {
            DialogueEventPayloadKind::EnvironmentSnapshotDiffed => {
                if self.diff_digest.is_none() {
                    return Err(ModelError::Missing("payload.diff_digest"));
                }
            }
            DialogueEventPayloadKind::EnvironmentMetricAnomaly => {
                if self.metrics_digest.is_none() {
                    return Err(ModelError::Missing("payload.metrics_digest"));
                }
                if let Some(score) = self.anomaly_score {
                    if score < 0.0 {
                        return Err(ModelError::Invariant("payload.anomaly_score"));
                    }
                }
            }
            _ => {}
        }
        Ok(())
    }
}

impl DialoguePayloadData for SelfReflectionPayload {
    fn validate_with_kind(&self, kind: DialogueEventPayloadKind) -> Result<(), ModelError> {
        if let Some(digest) = &self.insight_digest {
            ensure_not_blank(digest, "payload.insight_digest")?;
        }
        match kind {
            DialogueEventPayloadKind::SelfReflectionActionCommitted
            | DialogueEventPayloadKind::SelfReflectionArchived => {
                if self.related_event_id.is_none() {
                    return Err(ModelError::Missing("payload.related_event_id"));
                }
            }
            _ => {}
        }
        Ok(())
    }
}

impl DialogueEventPayload {
    pub fn message_ref(&self) -> Option<&MessagePointer> {
        match self {
            DialogueEventPayload::MessagePrimary(inner)
            | DialogueEventPayload::MessageEdited(inner)
            | DialogueEventPayload::MessageRegenerated(inner)
            | DialogueEventPayload::MessageSummaryPublished(inner)
            | DialogueEventPayload::MessageRecalled(inner)
            | DialogueEventPayload::MessagePinned(inner)
            | DialogueEventPayload::MessageFeedbackCaptured(inner)
            | DialogueEventPayload::MessageReactionAdded(inner)
            | DialogueEventPayload::MessageReactionRemoved(inner)
            | DialogueEventPayload::MessageThreadStarted(inner)
            | DialogueEventPayload::MessageThreadReplied(inner)
            | DialogueEventPayload::MessageAttachmentLinked(inner)
            | DialogueEventPayload::MessageAttachmentRemoved(inner)
            | DialogueEventPayload::MessageTranslated(inner)
            | DialogueEventPayload::MessageBroadcast(inner) => Some(&inner.message),
            _ => None,
        }
    }

    pub fn tool_invocation(&self) -> Option<&ToolInvocation> {
        match self {
            DialogueEventPayload::ToolPlanDrafted(inner)
            | DialogueEventPayload::ToolPlanValidated(inner)
            | DialogueEventPayload::ToolPlanRejected(inner)
            | DialogueEventPayload::ToolPlanCommitted(inner)
            | DialogueEventPayload::ToolPathDecided(inner)
            | DialogueEventPayload::ToolInvocationScheduled(inner)
            | DialogueEventPayload::ToolInvocationDispatched(inner)
            | DialogueEventPayload::ToolInvocationStarted(inner)
            | DialogueEventPayload::ToolInvocationProgressed(inner)
            | DialogueEventPayload::ToolInvocationCompleted(inner)
            | DialogueEventPayload::ToolInvocationFailed(inner)
            | DialogueEventPayload::ToolInvocationRetried(inner)
            | DialogueEventPayload::ToolResultRecorded(inner)
            | DialogueEventPayload::ToolResultAppended(inner)
            | DialogueEventPayload::ToolBarrierReached(inner)
            | DialogueEventPayload::ToolBarrierReleased(inner)
            | DialogueEventPayload::ToolBarrierTimeout(inner)
            | DialogueEventPayload::ToolRouteSwitched(inner) => inner.invocation.as_ref(),
            _ => None,
        }
    }

    pub fn tool_result(&self) -> Option<&ToolResult> {
        match self {
            DialogueEventPayload::ToolInvocationCompleted(inner)
            | DialogueEventPayload::ToolInvocationFailed(inner)
            | DialogueEventPayload::ToolResultRecorded(inner)
            | DialogueEventPayload::ToolResultAppended(inner)
            | DialogueEventPayload::ToolBarrierReached(inner)
            | DialogueEventPayload::ToolBarrierReleased(inner)
            | DialogueEventPayload::ToolBarrierTimeout(inner)
            | DialogueEventPayload::ToolRouteSwitched(inner) => inner.result.as_ref(),
            _ => None,
        }
    }

    pub fn self_reflection(&self) -> Option<&SelfReflectionRecord> {
        match self {
            DialogueEventPayload::SelfReflectionLogged(inner)
            | DialogueEventPayload::SelfReflectionHypothesisFormed(inner)
            | DialogueEventPayload::SelfReflectionActionCommitted(inner)
            | DialogueEventPayload::SelfReflectionScoreAdjusted(inner)
            | DialogueEventPayload::SelfReflectionArchived(inner) => Some(&inner.record),
            _ => None,
        }
    }

    pub fn content_digest_sha256(&self) -> Option<&String> {
        match self {
            DialogueEventPayload::MessagePrimary(inner)
            | DialogueEventPayload::MessageEdited(inner)
            | DialogueEventPayload::MessageRegenerated(inner)
            | DialogueEventPayload::MessageSummaryPublished(inner)
            | DialogueEventPayload::MessageRecalled(inner)
            | DialogueEventPayload::MessagePinned(inner)
            | DialogueEventPayload::MessageFeedbackCaptured(inner)
            | DialogueEventPayload::MessageReactionAdded(inner)
            | DialogueEventPayload::MessageReactionRemoved(inner)
            | DialogueEventPayload::MessageThreadStarted(inner)
            | DialogueEventPayload::MessageThreadReplied(inner)
            | DialogueEventPayload::MessageAttachmentLinked(inner)
            | DialogueEventPayload::MessageAttachmentRemoved(inner)
            | DialogueEventPayload::MessageTranslated(inner)
            | DialogueEventPayload::MessageBroadcast(inner) => inner
                .content_ref
                .as_ref()
                .and_then(|c| c.digest_sha256.as_ref()),
            DialogueEventPayload::ToolPlanDrafted(inner)
            | DialogueEventPayload::ToolPlanValidated(inner)
            | DialogueEventPayload::ToolPlanRejected(inner)
            | DialogueEventPayload::ToolPlanCommitted(inner)
            | DialogueEventPayload::ToolPathDecided(inner)
            | DialogueEventPayload::ToolInvocationScheduled(inner)
            | DialogueEventPayload::ToolInvocationDispatched(inner)
            | DialogueEventPayload::ToolInvocationStarted(inner)
            | DialogueEventPayload::ToolInvocationProgressed(inner)
            | DialogueEventPayload::ToolInvocationCompleted(inner)
            | DialogueEventPayload::ToolInvocationFailed(inner)
            | DialogueEventPayload::ToolInvocationRetried(inner)
            | DialogueEventPayload::ToolResultRecorded(inner)
            | DialogueEventPayload::ToolResultAppended(inner)
            | DialogueEventPayload::ToolBarrierReached(inner)
            | DialogueEventPayload::ToolBarrierReleased(inner)
            | DialogueEventPayload::ToolBarrierTimeout(inner)
            | DialogueEventPayload::ToolRouteSwitched(inner) =>
                inner.output_digest_sha256.as_ref(),
            DialogueEventPayload::SelfReflectionLogged(inner)
            | DialogueEventPayload::SelfReflectionHypothesisFormed(inner)
            | DialogueEventPayload::SelfReflectionActionCommitted(inner)
            | DialogueEventPayload::SelfReflectionScoreAdjusted(inner)
            | DialogueEventPayload::SelfReflectionArchived(inner) =>
                inner.insight_digest.as_ref(),
            _ => None,
        }
    }

    pub fn blob_ref(&self) -> Option<&String> {
        match self {
            DialogueEventPayload::MessagePrimary(inner)
            | DialogueEventPayload::MessageEdited(inner)
            | DialogueEventPayload::MessageRegenerated(inner)
            | DialogueEventPayload::MessageSummaryPublished(inner)
            | DialogueEventPayload::MessageRecalled(inner)
            | DialogueEventPayload::MessagePinned(inner)
            | DialogueEventPayload::MessageFeedbackCaptured(inner)
            | DialogueEventPayload::MessageReactionAdded(inner)
            | DialogueEventPayload::MessageReactionRemoved(inner)
            | DialogueEventPayload::MessageThreadStarted(inner)
            | DialogueEventPayload::MessageThreadReplied(inner)
            | DialogueEventPayload::MessageAttachmentLinked(inner)
            | DialogueEventPayload::MessageAttachmentRemoved(inner)
            | DialogueEventPayload::MessageTranslated(inner)
            | DialogueEventPayload::MessageBroadcast(inner) =>
                inner.content_ref.as_ref().and_then(|c| c.blob_ref.as_ref()),
            DialogueEventPayload::ToolPlanDrafted(inner)
            | DialogueEventPayload::ToolPlanValidated(inner)
            | DialogueEventPayload::ToolPlanRejected(inner)
            | DialogueEventPayload::ToolPlanCommitted(inner)
            | DialogueEventPayload::ToolPathDecided(inner)
            | DialogueEventPayload::ToolInvocationScheduled(inner)
            | DialogueEventPayload::ToolInvocationDispatched(inner)
            | DialogueEventPayload::ToolInvocationStarted(inner)
            | DialogueEventPayload::ToolInvocationProgressed(inner)
            | DialogueEventPayload::ToolInvocationCompleted(inner)
            | DialogueEventPayload::ToolInvocationFailed(inner)
            | DialogueEventPayload::ToolInvocationRetried(inner)
            | DialogueEventPayload::ToolResultRecorded(inner)
            | DialogueEventPayload::ToolResultAppended(inner)
            | DialogueEventPayload::ToolBarrierReached(inner)
            | DialogueEventPayload::ToolBarrierReleased(inner)
            | DialogueEventPayload::ToolBarrierTimeout(inner)
            | DialogueEventPayload::ToolRouteSwitched(inner) => inner.blob_ref.as_ref(),
            _ => None,
        }
    }

    pub fn evidence_pointer(&self) -> Option<&EvidencePointer> {
        match self {
            DialogueEventPayload::MessagePrimary(inner)
            | DialogueEventPayload::MessageEdited(inner)
            | DialogueEventPayload::MessageRegenerated(inner)
            | DialogueEventPayload::MessageSummaryPublished(inner)
            | DialogueEventPayload::MessageRecalled(inner)
            | DialogueEventPayload::MessagePinned(inner)
            | DialogueEventPayload::MessageFeedbackCaptured(inner)
            | DialogueEventPayload::MessageReactionAdded(inner)
            | DialogueEventPayload::MessageReactionRemoved(inner)
            | DialogueEventPayload::MessageThreadStarted(inner)
            | DialogueEventPayload::MessageThreadReplied(inner)
            | DialogueEventPayload::MessageAttachmentLinked(inner)
            | DialogueEventPayload::MessageAttachmentRemoved(inner)
            | DialogueEventPayload::MessageTranslated(inner)
            | DialogueEventPayload::MessageBroadcast(inner) => inner
                .content_ref
                .as_ref()
                .and_then(|c| c.evidence.as_ref()),
            DialogueEventPayload::ToolPlanDrafted(inner)
            | DialogueEventPayload::ToolPlanValidated(inner)
            | DialogueEventPayload::ToolPlanRejected(inner)
            | DialogueEventPayload::ToolPlanCommitted(inner)
            | DialogueEventPayload::ToolPathDecided(inner)
            | DialogueEventPayload::ToolInvocationScheduled(inner)
            | DialogueEventPayload::ToolInvocationDispatched(inner)
            | DialogueEventPayload::ToolInvocationStarted(inner)
            | DialogueEventPayload::ToolInvocationProgressed(inner)
            | DialogueEventPayload::ToolInvocationCompleted(inner)
            | DialogueEventPayload::ToolInvocationFailed(inner)
            | DialogueEventPayload::ToolInvocationRetried(inner)
            | DialogueEventPayload::ToolResultRecorded(inner)
            | DialogueEventPayload::ToolResultAppended(inner)
            | DialogueEventPayload::ToolBarrierReached(inner)
            | DialogueEventPayload::ToolBarrierReleased(inner)
            | DialogueEventPayload::ToolBarrierTimeout(inner)
            | DialogueEventPayload::ToolRouteSwitched(inner) =>
                inner.evidence.first(),
            _ => None,
        }
    }
}
