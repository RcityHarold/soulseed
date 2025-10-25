use soulseed_agi_core_models::{DialogueEvent, DialogueEventEnhancements, EventId};
use std::collections::HashMap;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;
use thiserror::Error;

#[derive(Error, Debug, Clone, PartialEq, Eq)]
pub enum EnhancementError {
    #[error("generation failed: {0}")]
    Generation(String),
    #[error("persistence failed: {0}")]
    Persistence(String),
}

#[derive(Error, Debug)]
pub enum PipelineError {
    #[error("max_retries must be >= 1")]
    InvalidRetryBudget,
    #[error("job for event {0:?} already scheduled")]
    Duplicate(EventId),
    #[error("pipeline queue closed")]
    QueueClosed,
}

#[derive(Error, Debug)]
pub enum EventWriteError {
    #[error("base layer write failed: {0}")]
    BaseFailed(String),
    #[error(transparent)]
    Pipeline(#[from] PipelineError),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum JobStatus {
    Pending { attempts: u8 },
    Running { attempts: u8 },
    Retrying { attempts: u8, error: EnhancementError },
    Succeeded { attempts: u8 },
    RolledBack { attempts: u8, error: EnhancementError },
}

pub trait EnhancementGenerator: Send + Sync + 'static {
    fn generate(
        &self,
        event: &DialogueEvent,
    ) -> Result<DialogueEventEnhancements, EnhancementError>;

    fn rollback(&self, event: &DialogueEvent, error: &EnhancementError);
}

pub trait EnhancementSink: Send + Sync + 'static {
    fn persist(
        &self,
        event: &DialogueEvent,
        enhancements: &DialogueEventEnhancements,
    ) -> Result<(), EnhancementError>;

    fn mark_failed(&self, event: &DialogueEvent, error: &EnhancementError);
}

pub trait BaseEventWriter: Send + Sync + 'static {
    fn persist(&self, event: &DialogueEvent) -> Result<(), String>;
}

#[derive(Clone)]
struct EnhancementJob {
    event: DialogueEvent,
    attempts: u8,
    max_retries: u8,
}

enum PipelineCommand {
    Enqueue(EnhancementJob),
    Shutdown,
}

pub struct EnhancementPipeline<G, S>
where
    G: EnhancementGenerator,
    S: EnhancementSink,
{
    tx: Sender<PipelineCommand>,
    state: Arc<Mutex<HashMap<EventId, JobStatus>>>,
    handles: Vec<JoinHandle<()>>,
    _generator: Arc<G>,
    _sink: Arc<S>,
}

impl<G, S> EnhancementPipeline<G, S>
where
    G: EnhancementGenerator,
    S: EnhancementSink,
{
    pub fn new(generator: Arc<G>, sink: Arc<S>) -> Self {
        let (tx, rx) = mpsc::channel::<PipelineCommand>();
        let state = Arc::new(Mutex::new(HashMap::new()));
        let worker = Self::spawn_worker(
            rx,
            tx.clone(),
            Arc::clone(&generator),
            Arc::clone(&sink),
            Arc::clone(&state),
        );
        Self {
            tx,
            state,
            handles: vec![worker],
            _generator: generator,
            _sink: sink,
        }
    }

    pub fn enqueue(
        &self,
        event: DialogueEvent,
        max_retries: u8,
    ) -> Result<EventId, PipelineError> {
        if max_retries == 0 {
            return Err(PipelineError::InvalidRetryBudget);
        }
        let event_id = event.base.event_id;
        {
            let mut guard = self.state.lock().unwrap();
            if guard.contains_key(&event_id) {
                return Err(PipelineError::Duplicate(event_id));
            }
            guard.insert(event_id, JobStatus::Pending { attempts: 0 });
        }
        let job = EnhancementJob {
            event,
            attempts: 0,
            max_retries,
        };
        self.tx
            .send(PipelineCommand::Enqueue(job))
            .map_err(|_| PipelineError::QueueClosed)?;
        Ok(event_id)
    }

    pub fn job_status(&self, event_id: &EventId) -> Option<JobStatus> {
        self.state.lock().unwrap().get(event_id).cloned()
    }

    pub fn state_snapshot(&self) -> HashMap<EventId, JobStatus> {
        self.state.lock().unwrap().clone()
    }

    fn spawn_worker(
        rx: Receiver<PipelineCommand>,
        tx: Sender<PipelineCommand>,
        generator: Arc<G>,
        sink: Arc<S>,
        state: Arc<Mutex<HashMap<EventId, JobStatus>>>,
    ) -> JoinHandle<()> {
        thread::spawn(move || {
            while let Ok(cmd) = rx.recv() {
                match cmd {
                    PipelineCommand::Enqueue(mut job) => {
                        let event_id = job.event.base.event_id;
                        let attempt_no = job.attempts.saturating_add(1);
                        {
                            let mut guard = state.lock().unwrap();
                            guard.insert(event_id, JobStatus::Running { attempts: attempt_no });
                        }
                        let result = generator.generate(&job.event).and_then(|enhancements| {
                            sink.persist(&job.event, &enhancements)?;
                            Ok(())
                        });

                        match result {
                            Ok(()) => {
                                let mut guard = state.lock().unwrap();
                                guard.insert(event_id, JobStatus::Succeeded { attempts: attempt_no });
                            }
                            Err(err) => {
                                if attempt_no >= job.max_retries {
                                    generator.rollback(&job.event, &err);
                                    sink.mark_failed(&job.event, &err);
                                    let mut guard = state.lock().unwrap();
                                    guard.insert(
                                        event_id,
                                        JobStatus::RolledBack {
                                            attempts: attempt_no,
                                            error: err,
                                        },
                                    );
                                } else {
                                    job.attempts = attempt_no;
                                    {
                                        let mut guard = state.lock().unwrap();
                                        guard.insert(
                                            event_id,
                                            JobStatus::Retrying {
                                                attempts: attempt_no,
                                                error: err.clone(),
                                            },
                                        );
                                    }
                                    thread::sleep(Duration::from_millis(50));
                                    if tx.send(PipelineCommand::Enqueue(job)).is_err() {
                                        let mut guard = state.lock().unwrap();
                                        guard.insert(
                                            event_id,
                                            JobStatus::RolledBack {
                                                attempts: attempt_no,
                                                error: EnhancementError::Persistence(
                                                    "retry queue closed".into(),
                                                ),
                                            },
                                        );
                                    }
                                }
                            }
                        }
                    }
                    PipelineCommand::Shutdown => break,
                }
            }
        })
    }
}

impl<G, S> Drop for EnhancementPipeline<G, S>
where
    G: EnhancementGenerator,
    S: EnhancementSink,
{
    fn drop(&mut self) {
        for _ in &self.handles {
            let _ = self.tx.send(PipelineCommand::Shutdown);
        }
        for handle in self.handles.drain(..) {
            let _ = handle.join();
        }
    }
}

pub struct EventWriteCoordinator<B, G, S>
where
    B: BaseEventWriter,
    G: EnhancementGenerator,
    S: EnhancementSink,
{
    base_writer: Arc<B>,
    pipeline: EnhancementPipeline<G, S>,
}

impl<B, G, S> EventWriteCoordinator<B, G, S>
where
    B: BaseEventWriter,
    G: EnhancementGenerator,
    S: EnhancementSink,
{
    pub fn new(base_writer: Arc<B>, generator: Arc<G>, sink: Arc<S>) -> Self {
        let pipeline = EnhancementPipeline::new(generator, sink);
        Self {
            base_writer,
            pipeline,
        }
    }

    pub fn write_event(
        &self,
        event: DialogueEvent,
        max_retries: u8,
    ) -> Result<EventId, EventWriteError> {
        self.base_writer
            .persist(&event)
            .map_err(EventWriteError::BaseFailed)?;
        let event_id = event.base.event_id;
        self.pipeline.enqueue(event, max_retries)?;
        Ok(event_id)
    }

    pub fn pipeline(&self) -> &EnhancementPipeline<G, S> {
        &self.pipeline
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use soulseed_agi_core_models::dialogue_event::LegacyEnhancementRecord;
    use soulseed_agi_core_models::{
        dialogue_event::{migrate_legacy_enhancements, DialogueEventBase, DialogueEventPayload},
        enums::{ConversationScenario, DialogueEventType},
        ids::{EventId, SessionId, TenantId},
        AccessClass, CorrelationId, EnvelopeHead, Snapshot, Subject, TraceId,
    };
    use std::sync::atomic::{AtomicUsize, Ordering};
    use time::OffsetDateTime;
    use uuid::Uuid;

    fn dummy_event(event_id: u64) -> DialogueEvent {
        let base = DialogueEventBase {
            tenant_id: TenantId::from_raw_unchecked(1),
            event_id: EventId::from_raw_unchecked(event_id),
            session_id: SessionId::from_raw_unchecked(1),
            subject: Subject::System,
            participants: Vec::new(),
            head: EnvelopeHead {
                envelope_id: Uuid::nil(),
                trace_id: TraceId("trace".into()),
                correlation_id: CorrelationId("corr".into()),
                config_snapshot_hash:
                    "a7f0b2044a9be4f65d8c7e1f9c2d4b5a6c7d8e9f0a1b2c3d4e5f6a7b8c9d0e1f".into(),
                config_snapshot_version: 1,
            },
            snapshot: Snapshot {
                schema_v: 1,
                created_at: OffsetDateTime::UNIX_EPOCH,
            },
            timestamp_ms: 100,
            scenario: ConversationScenario::HumanToAi,
            event_type: DialogueEventType::System,
            stage_hint: None,
            access_class: AccessClass::Public,
            provenance: None,
            sequence_number: 1,
            ac_id: None,
            ic_sequence: None,
            parent_ac_id: None,
            config_version: Some("1.0.0".into()),
            trigger_event_id: None,
            supersedes: None,
            superseded_by: None,
        };
        DialogueEvent {
            base,
            payload: DialogueEventPayload::SystemNotificationDispatched(
                soulseed_agi_core_models::dialogue_event::SystemPayload {
                    category: "test".into(),
                    related_service: None,
                    detail: serde_json::Value::Null,
                },
            ),
            enhancements: None,
            metadata: serde_json::Value::Null,
            #[cfg(feature = "vectors-extra")]
            vectors: Default::default(),
        }
    }

    struct TestGenerator {
        failures_before_success: AtomicUsize,
        rollback_calls: AtomicUsize,
    }

    impl TestGenerator {
        fn new(failures_before_success: usize) -> Self {
            Self {
                failures_before_success: AtomicUsize::new(failures_before_success),
                rollback_calls: AtomicUsize::new(0),
            }
        }

        fn rollback_count(&self) -> usize {
            self.rollback_calls.load(Ordering::SeqCst)
        }
    }

    impl EnhancementGenerator for TestGenerator {
        fn generate(
            &self,
            event: &DialogueEvent,
        ) -> Result<DialogueEventEnhancements, EnhancementError> {
            let remaining = self
                .failures_before_success
                .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |value| {
                    value.checked_sub(1)
                });
            match remaining {
                Ok(_) => Err(EnhancementError::Generation(format!(
                    "simulated failure for {:?}",
                    event.base.event_id
                ))),
                Err(_) => {
                    let record = LegacyEnhancementRecord {
                        time_window: Some("alpha".into()),
                        temporal_pattern_id: None,
                        causal_links: Vec::new(),
                        content_embedding: None,
                        context_embedding: None,
                        decision_embedding: None,
                        embedding_meta: None,
                        concept_vector: None,
                        semantic_cluster_id: None,
                        cluster_method: None,
                        concept_distance_to_goal: None,
                        real_time_priority: None,
                        notification_targets: None,
                        live_stream_id: None,
                        growth_stage: None,
                        processing_latency_ms: None,
                        influence_score: None,
                        community_impact: None,
                    };
                    Ok(migrate_legacy_enhancements(record, event.base.timestamp_ms))
                }
            }
        }

        fn rollback(&self, _event: &DialogueEvent, _error: &EnhancementError) {
            self.rollback_calls.fetch_add(1, Ordering::SeqCst);
        }
    }

    struct MemorySink {
        persisted: Mutex<Vec<EventId>>,
        failures: Mutex<Vec<EventId>>,
    }

    impl MemorySink {
        fn new() -> Self {
            Self {
                persisted: Mutex::new(Vec::new()),
                failures: Mutex::new(Vec::new()),
            }
        }
    }

    impl EnhancementSink for MemorySink {
        fn persist(
            &self,
            event: &DialogueEvent,
            _enhancements: &DialogueEventEnhancements,
        ) -> Result<(), EnhancementError> {
            self.persisted
                .lock()
                .unwrap()
                .push(event.base.event_id);
            Ok(())
        }

        fn mark_failed(&self, event: &DialogueEvent, _error: &EnhancementError) {
            self.failures.lock().unwrap().push(event.base.event_id);
        }
    }

    struct MemoryBaseWriter {
        should_fail: bool,
        writes: Mutex<Vec<EventId>>,
    }

    impl MemoryBaseWriter {
        fn new(should_fail: bool) -> Self {
            Self {
                should_fail,
                writes: Mutex::new(Vec::new()),
            }
        }
    }

    impl BaseEventWriter for MemoryBaseWriter {
        fn persist(&self, event: &DialogueEvent) -> Result<(), String> {
            if self.should_fail {
                Err("primary store error".into())
            } else {
                self.writes.lock().unwrap().push(event.base.event_id);
                Ok(())
            }
        }
    }

    fn wait_for_status<G, S>(
        pipeline: &EnhancementPipeline<G, S>,
        event_id: EventId,
        predicate: impl Fn(&JobStatus) -> bool,
    ) where
        G: EnhancementGenerator,
        S: EnhancementSink,
    {
        for _ in 0..40 {
            if let Some(status) = pipeline.job_status(&event_id) {
                if predicate(&status) {
                    return;
                }
            }
            thread::sleep(Duration::from_millis(25));
        }
        panic!("job did not reach expected status");
    }

    #[test]
    fn pipeline_completes_successfully_after_retries() {
        let generator = Arc::new(TestGenerator::new(2));
        let sink = Arc::new(MemorySink::new());
        let pipeline = EnhancementPipeline::new(Arc::clone(&generator), Arc::clone(&sink));

        let event = dummy_event(10);
        let event_id = pipeline.enqueue(event, 3).expect("enqueue ok");

        wait_for_status(&pipeline, event_id, |status| {
            matches!(status, JobStatus::Succeeded { attempts: 3 })
        });

        let persisted = sink.persisted.lock().unwrap();
        assert_eq!(persisted.len(), 1);
        assert_eq!(persisted[0], event_id);
        assert_eq!(generator.rollback_count(), 0);
    }

    #[test]
    fn pipeline_rolls_back_after_max_attempts() {
        let generator = Arc::new(TestGenerator::new(usize::MAX));
        let sink = Arc::new(MemorySink::new());
        let pipeline = EnhancementPipeline::new(Arc::clone(&generator), Arc::clone(&sink));

        let event = dummy_event(11);
        let event_id = pipeline.enqueue(event, 2).expect("enqueue ok");

        wait_for_status(&pipeline, event_id, |status| {
            matches!(status, JobStatus::RolledBack { attempts: 2, .. })
        });

        assert_eq!(generator.rollback_count(), 1);
        assert!(sink.persisted.lock().unwrap().is_empty());
        assert_eq!(sink.failures.lock().unwrap().len(), 1);
    }

    #[test]
    fn coordinator_stops_on_base_write_error() {
        let generator = Arc::new(TestGenerator::new(0));
        let sink = Arc::new(MemorySink::new());
        let base_writer = Arc::new(MemoryBaseWriter::new(true));
        let coordinator = EventWriteCoordinator::new(
            Arc::clone(&base_writer),
            Arc::clone(&generator),
            Arc::clone(&sink),
        );

        let event = dummy_event(12);
        let err = coordinator.write_event(event, 2).unwrap_err();
        matches!(err, EventWriteError::BaseFailed(_));
        assert!(base_writer.writes.lock().unwrap().is_empty());
        assert!(sink.persisted.lock().unwrap().is_empty());
    }

    #[test]
    fn coordinator_enqueues_after_base_write() {
        let generator = Arc::new(TestGenerator::new(0));
        let sink = Arc::new(MemorySink::new());
        let base_writer = Arc::new(MemoryBaseWriter::new(false));
        let coordinator = EventWriteCoordinator::new(
            Arc::clone(&base_writer),
            Arc::clone(&generator),
            Arc::clone(&sink),
        );

        let event = dummy_event(13);
        let event_id = coordinator.write_event(event, 2).expect("write ok");

        wait_for_status(coordinator.pipeline(), event_id, |status| {
            matches!(status, JobStatus::Succeeded { .. })
        });

        assert_eq!(base_writer.writes.lock().unwrap().len(), 1);
        assert_eq!(sink.persisted.lock().unwrap().len(), 1);
    }
}
