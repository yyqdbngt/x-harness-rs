//! Opt-in scheduler experiments, not live-model or coding-quality benchmarks.
use super::*;
use crate::{DurableLoopAgentRuntime, HostConfig, PermissionPreset, SessionToolFactory};
use std::{
    path::{Path, PathBuf},
    sync::{atomic::AtomicUsize, Mutex},
    time::{Instant, SystemTime, UNIX_EPOCH},
};
use xharness_agent::{InboxProjection, MemoryLeaseManager};
use xharness_core::{
    FinishReason, IdentityContextPolicy, ModelProvider, ProviderError, ProviderEvent,
    ProviderRequest, ProviderStream,
};
use xharness_session::{Store, TurnEndReason};
use xharness_session_jsonl::JsonlSessionStore;
use xharness_tools::{ToolExecutor, ToolRegistry};

#[derive(Default)]
struct Counter {
    active: AtomicUsize,
    peak: AtomicUsize,
    calls: AtomicUsize,
}
impl Counter {
    fn enter(self: &Arc<Self>) -> Active {
        self.calls.fetch_add(1, Ordering::SeqCst);
        let n = self.active.fetch_add(1, Ordering::SeqCst) + 1;
        self.peak.fetch_max(n, Ordering::SeqCst);
        Active(self.clone())
    }
}
struct Active(Arc<Counter>);
impl Drop for Active {
    fn drop(&mut self) {
        self.0.active.fetch_sub(1, Ordering::SeqCst);
    }
}

struct Fixture {
    profile: String,
    model: Arc<Counter>,
    tool: Arc<Counter>,
    first_steps: Mutex<BTreeMap<String, Instant>>,
    provider_slots: Arc<tokio::sync::Semaphore>,
    gate: CancellationToken,
}
impl Fixture {
    fn new(profile: &str) -> Arc<Self> {
        Arc::new(Self {
            profile: profile.into(),
            model: Arc::default(),
            tool: Arc::default(),
            first_steps: Mutex::default(),
            provider_slots: Arc::new(tokio::sync::Semaphore::new(if profile == "provider_cap2" {
                2
            } else {
                16
            })),
            gate: CancellationToken::new(),
        })
    }
}
#[async_trait]
impl ModelProvider for Fixture {
    fn provider_name(&self) -> &str {
        "capacity-fixture"
    }
    async fn stream(
        &self,
        request: ProviderRequest,
        _cancel: CancellationToken,
    ) -> Result<ProviderStream, ProviderError> {
        if request.step == 1 {
            let task = request.messages.last().unwrap().content.clone();
            self.first_steps
                .lock()
                .unwrap()
                .insert(task, Instant::now());
        }
        let permit = self.provider_slots.clone().acquire_owned().await.unwrap();
        let active = self.model.enter();
        let tools = self.profile == "tool_wait";
        let blocked = self.profile == "blocked";
        let gate = self.gate.clone();
        Ok(Box::pin(async_stream::stream! {
            let _permit = permit;
            let _active = active;
            if blocked {
                gate.cancelled().await;
            } else {
                tokio::time::sleep(Duration::from_millis(if tools { 20 } else { 250 })).await;
            }
            if tools && request.step == 1 {
                yield Ok(ProviderEvent::ToolCallDelta {
                    index: 0, id: "fixture-call".into(), name: "file_roundtrip".into(),
                    arguments_delta: "{}".into(),
                });
                yield Ok(ProviderEvent::Completed {
                    finish_reason: Some(FinishReason::ToolCalls), usage: None, provider_items: vec![],
                });
            } else {
                yield Ok(ProviderEvent::TextDelta("fixture completed".into()));
                yield Ok(ProviderEvent::Completed {
                    finish_reason: Some(FinishReason::Stop), usage: None, provider_items: vec![],
                });
            }
        }))
    }
}

struct FileTools {
    root: PathBuf,
    counter: Arc<Counter>,
}
#[async_trait]
impl SessionToolFactory for FileTools {
    async fn executor(
        &self,
        session: &str,
        _: &str,
        _: PermissionPreset,
    ) -> Result<ToolExecutor, String> {
        let registry = Arc::new(ToolRegistry::new());
        let path = self.root.join(format!("{session}.fixture"));
        let counter = self.counter.clone();
        registry
            .register(
                ToolSpec::new(
                    ToolDefinition::new(
                        "file_roundtrip",
                        "Isolated fixture I/O, not a coding benchmark",
                        json!({"type":"object","properties":{},"additionalProperties":false}),
                    ),
                    move |_| {
                        let path = path.clone();
                        let counter = counter.clone();
                        async move {
                            let _active = counter.enter();
                            let payload = vec![0x5a; 256 * 1024];
                            tokio::fs::write(&path, &payload)
                                .await
                                .map_err(|e| ToolHandlerError::new(e.to_string()))?;
                            tokio::time::sleep(Duration::from_millis(200)).await;
                            let actual = tokio::fs::read(&path)
                                .await
                                .map_err(|e| ToolHandlerError::new(e.to_string()))?;
                            if actual != payload {
                                return Err(ToolHandlerError::new("fixture file corruption"));
                            }
                            Ok(ToolOutput::text("verified 262144 bytes"))
                        }
                    },
                )
                .with_concurrency(ToolConcurrency::Parallel),
            )
            .await
            .map_err(|e| e.to_string())?;
        Ok(ToolExecutor::new(registry))
    }
}

fn root(label: &str) -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = std::env::temp_dir().join(format!(
        "xharness-capacity-{label}-{}-{nonce}",
        std::process::id()
    ));
    std::fs::create_dir(&root).unwrap();
    root
}
fn capacity() -> usize {
    let n = std::env::var("XHARNESS_TEST_CAPACITY")
        .unwrap()
        .parse()
        .unwrap();
    assert!(matches!(n, 2 | 4 | 8));
    n
}
fn setup(root: &Path, n: usize, fixture: Arc<Fixture>) -> (Arc<BasicHost>, Arc<dyn Store>) {
    let store: Arc<dyn Store> = Arc::new(JsonlSessionStore::new(root.join("sessions")).unwrap());
    let tools = Arc::new(FileTools {
        root: root.into(),
        counter: fixture.tool.clone(),
    });
    let runtime = DurableLoopAgentRuntime::new(
        "capacity-fixture",
        "scripted",
        Some(fixture),
        tools,
        Arc::new(IdentityContextPolicy),
        store.clone(),
        Arc::new(MemoryLeaseManager::default()),
        2048,
    )
    .with_experiment_capacity(n);
    let mut config = HostConfig::new(root);
    config.provider_id = "capacity-fixture".into();
    config.model_id = "scripted".into();
    (
        BasicHost::with_agent_runtime(config, Arc::new(runtime)),
        store,
    )
}
async fn parent(host: &BasicHost) {
    host.session_create(&json!({"sessionId":"parent"}))
        .await
        .unwrap();
    host.set_dispatch_paused("parent", true).await.unwrap();
}
async fn start(host: &BasicHost, task: &str) -> String {
    host.execute_agent(
        "parent",
        task,
        AgentOperation::Start {
            task: task.into(),
            label: None,
        },
    )
    .await
    .unwrap()["agent_id"]
        .as_str()
        .unwrap()
        .into()
}
async fn settled(host: &BasicHost, ids: &[String]) {
    tokio::time::timeout(Duration::from_secs(30), async {
        loop {
            if ids.iter().all(|id| {
                !host
                    .state
                    .try_read()
                    .map(|s| s.sessions[id].running)
                    .unwrap_or(true)
            }) {
                break;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .expect("children did not settle");
}
async fn wait_calls(fixture: &Fixture, n: usize) {
    tokio::time::timeout(Duration::from_secs(10), async {
        while fixture.model.calls.load(Ordering::SeqCst) < n {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .expect("capacity not filled");
}
fn percentile(values: &mut [f64], percentile: usize) -> f64 {
    values.sort_by(f64::total_cmp);
    values[(values.len() * percentile).div_ceil(100).saturating_sub(1)]
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "opt-in isolated capacity experiment; requires XHARNESS_TEST_CAPACITY/PROFILE"]
async fn throughput() {
    let n = capacity();
    let profile = std::env::var("XHARNESS_TEST_PROFILE").unwrap();
    assert!(matches!(
        profile.as_str(),
        "model_wait" | "tool_wait" | "provider_cap2"
    ));
    let root = root("throughput");
    let fixture = Fixture::new(&profile);
    let (host, store) = setup(&root, n, fixture.clone());
    parent(&host).await;
    let begin = Instant::now();
    let mut admissions = BTreeMap::new();
    let mut ids = vec![];
    for i in 0..12 {
        let task = format!("fixture-task-{i}");
        admissions.insert(task.clone(), Instant::now());
        ids.push(start(&host, &task).await);
    }
    settled(&host, &ids).await;
    let elapsed = begin.elapsed().as_secs_f64() * 1000.0;
    assert_eq!(fixture.model.active.load(Ordering::SeqCst), 0);
    assert_eq!(fixture.tool.active.load(Ordering::SeqCst), 0);
    let peak = fixture.model.peak.load(Ordering::SeqCst);
    assert!(peak > 0 && peak <= if profile == "provider_cap2" { 2 } else { n });
    assert_eq!(
        fixture.model.calls.load(Ordering::SeqCst),
        if profile == "tool_wait" { 24 } else { 12 }
    );
    assert_eq!(
        fixture.tool.calls.load(Ordering::SeqCst),
        if profile == "tool_wait" { 12 } else { 0 }
    );
    assert!(fixture.tool.peak.load(Ordering::SeqCst) <= n);
    for id in &ids {
        let session = store.load(id).await.unwrap().unwrap();
        assert_eq!(
            session
                .events()
                .iter()
                .filter(|e| matches!(
                    e.data(),
                    EventData::TurnEnd {
                        reason: TurnEndReason::Completed,
                        ..
                    }
                ))
                .count(),
            1
        );
        assert!(!InboxProjection::from_session(&session)
            .unwrap()
            .has_pending());
        host.deliver_child_settlements(id).await.unwrap();
        host.deliver_child_settlements(id).await.unwrap();
        if profile == "tool_wait" {
            assert_eq!(
                tokio::fs::read(root.join(format!("{id}.fixture")))
                    .await
                    .unwrap(),
                vec![0x5a; 256 * 1024]
            );
        }
    }
    assert_eq!(host.state.read().await.sessions["parent"].queue.len(), 12);
    host.agent_runtime.shutdown(Duration::from_secs(3)).await;
    // Reopen from disk, not the previous store's cache; settlements stay exactly once.
    let resumed_fixture = Fixture::new(&profile);
    let (resumed, reopened) = setup(&root, n, resumed_fixture.clone());
    resumed.restore_from_store(reopened).await.unwrap();
    for id in &ids {
        resumed.deliver_child_settlements(id).await.unwrap();
    }
    assert_eq!(
        resumed.state.read().await.sessions["parent"].queue.len(),
        12
    );
    assert_eq!(resumed_fixture.model.calls.load(Ordering::SeqCst), 0);
    resumed.agent_runtime.shutdown(Duration::from_secs(3)).await;
    let mut waits: Vec<f64> = fixture
        .first_steps
        .lock()
        .unwrap()
        .iter()
        .map(|(task, start)| start.duration_since(admissions[task]).as_secs_f64() * 1000.0)
        .collect();
    assert_eq!(waits.len(), 12);
    println!(
        "CAPACITY_RESULT {}",
        json!({
            "kind":"throughput", "capacity":n, "profile":profile, "tasks":12,
            "elapsed_ms":elapsed, "first_step_wait_p50_ms":percentile(&mut waits, 50),
            "first_step_wait_p95_ms":percentile(&mut waits, 95), "peak_model_streams":peak,
            "peak_tools":fixture.tool.peak.load(Ordering::SeqCst), "settlements_after_reopen":12,
            "fixture_root":root,
        })
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "opt-in cancellation/admission experiment; requires XHARNESS_TEST_CAPACITY"]
async fn cancellation_and_admission() {
    let n = capacity();
    let root = root("cancel");
    let fixture = Fixture::new("blocked");
    let (host, store) = setup(&root, n, fixture.clone());
    parent(&host).await;
    let mut ids = vec![];
    for i in 0..n {
        ids.push(start(&host, &format!("active-{i}")).await);
    }
    wait_calls(&fixture, n).await;
    for i in n..16 {
        ids.push(start(&host, &format!("queued-{i}")).await);
    }
    let error = host
        .execute_agent(
            "parent",
            "overflow",
            AgentOperation::Start {
                task: "overflow".into(),
                label: None,
            },
        )
        .await
        .unwrap_err();
    assert!(error.contains("admission queue is full"), "{error}");
    assert_eq!(host.state.read().await.sessions.len(), 17);
    let queued = ids.last().unwrap();
    let begin = Instant::now();
    tokio::time::timeout(
        Duration::from_secs(2),
        host.execute_agent(
            "parent",
            "stop-queued",
            AgentOperation::Stop {
                agent_id: queued.clone(),
            },
        ),
    )
    .await
    .expect("queued cancellation hung")
    .unwrap();
    settled(&host, std::slice::from_ref(queued)).await;
    let cancel_ms = begin.elapsed().as_secs_f64() * 1000.0;
    let session = store.load(queued).await.unwrap().unwrap();
    assert!(!session
        .events()
        .iter()
        .any(|e| matches!(e.data(), EventData::TurnStart { .. })));
    assert!(InboxProjection::from_session(&session)
        .unwrap()
        .has_pending());
    assert_eq!(fixture.model.calls.load(Ordering::SeqCst), n);
    // Cancelling an active stream must drop its permit and admit a queued child.
    tokio::time::timeout(
        Duration::from_secs(2),
        host.execute_agent(
            "parent",
            "stop-active",
            AgentOperation::Stop {
                agent_id: ids[0].clone(),
            },
        ),
    )
    .await
    .expect("active cancellation hung")
    .unwrap();
    wait_calls(&fixture, n + 1).await;
    fixture.gate.cancel();
    settled(&host, &ids).await;
    let replacement = start(&host, "reuse-after-drain").await;
    settled(&host, &[replacement]).await;
    assert_eq!(fixture.model.calls.load(Ordering::SeqCst), 16);
    assert_eq!(fixture.model.active.load(Ordering::SeqCst), 0);
    assert_eq!(fixture.model.peak.load(Ordering::SeqCst), n);
    host.agent_runtime.shutdown(Duration::from_secs(3)).await;
    println!(
        "CAPACITY_RESULT {}",
        json!({"kind":"safety", "capacity":n,
        "queued_cancel_ms":cancel_ms, "admission_limit":16, "peak_model_streams":n,
        "pending_input_preserved":true, "active_cancel_released_slot":true})
    );
}
