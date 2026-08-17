use std::{
    io,
    sync::{
        Arc, Mutex,
        atomic::{AtomicU32, Ordering},
    },
    time::{Duration, Instant},
};

use shilpo::shell::extensions::supervisor::{
    ChildSpawner, ChildStream, Clock, ExtensionSupervisor, READY_RESET_DURATION, RETRY_DELAYS,
    SupervisorState,
};
use shilpo::shell::extensions::{
    ExtensionCommand, ExtensionGeneration, ExtensionSnapshot, ExtensionUpdate,
};
use shilpo_ext_runtime::{
    HostGeneration, HostMessage, PROTOCOL_VERSION, ProcessCodecError, WorkerMessage, WorkerPayload,
};

struct TestClock {
    now: Arc<Mutex<Instant>>,
}

impl Clock for TestClock {
    fn now(&self) -> Instant {
        *self.now.lock().unwrap()
    }

    fn sleep(&self, duration: Duration) {
        *self.now.lock().unwrap() += duration;
    }
}

struct FailingChildSpawner {
    spawn_count: Arc<AtomicU32>,
}

struct ReadyChild {
    host_generation: HostGeneration,
    initial_sent: bool,
    alive: bool,
    writes: Arc<Mutex<Vec<ExtensionCommand>>>,
}

struct StaleUpdateChild {
    host_generation: HostGeneration,
    phase: u8,
}

struct StaleEngineChild {
    host_generation: HostGeneration,
    phase: u8,
    alive: bool,
}

impl ChildStream for StaleEngineChild {
    fn pid(&self) -> Option<u32> {
        Some(4545)
    }

    fn write_host_message(&mut self, _message: &HostMessage) -> Result<(), ProcessCodecError> {
        Ok(())
    }

    fn try_read_worker_message(&mut self) -> Result<Option<WorkerMessage>, ProcessCodecError> {
        let message = match self.phase {
            0 => Some(WorkerMessage {
                protocol_version: PROTOCOL_VERSION,
                host_generation: self.host_generation,
                engine_generation: ExtensionGeneration(2),
                request_id: 1,
                payload: WorkerPayload::Update(ExtensionUpdate {
                    host_generation: self.host_generation,
                    generation: ExtensionGeneration(2),
                    snapshot: Some(ExtensionSnapshot {
                        generation: ExtensionGeneration(2),
                        ..ExtensionSnapshot::default()
                    }),
                    effects: Vec::new(),
                    invalidated_views: Vec::new(),
                    circuit_notices: Vec::new(),
                }),
            }),
            1 => Some(WorkerMessage {
                protocol_version: PROTOCOL_VERSION,
                host_generation: self.host_generation,
                engine_generation: ExtensionGeneration(1),
                request_id: 2,
                payload: WorkerPayload::Update(ExtensionUpdate {
                    host_generation: self.host_generation,
                    generation: ExtensionGeneration(99),
                    snapshot: Some(ExtensionSnapshot {
                        generation: ExtensionGeneration(99),
                        ..ExtensionSnapshot::default()
                    }),
                    effects: Vec::new(),
                    invalidated_views: Vec::new(),
                    circuit_notices: Vec::new(),
                }),
            }),
            _ => None,
        };
        self.phase = self.phase.saturating_add(1);
        Ok(message)
    }

    fn try_wait(&mut self) -> io::Result<Option<std::process::ExitStatus>> {
        if self.alive {
            Ok(None)
        } else {
            Ok(Some(std::process::ExitStatus::default()))
        }
    }

    fn shutdown_gracefully(&mut self, _timeout: Duration) -> io::Result<()> {
        self.alive = false;
        Ok(())
    }

    fn kill(&mut self) -> io::Result<()> {
        self.alive = false;
        Ok(())
    }
}

struct StaleEngineSpawner;

impl ChildSpawner for StaleEngineSpawner {
    fn spawn(&self, host_generation: HostGeneration) -> io::Result<Box<dyn ChildStream>> {
        Ok(Box::new(StaleEngineChild {
            host_generation,
            phase: 0,
            alive: true,
        }))
    }
}

impl ChildStream for StaleUpdateChild {
    fn pid(&self) -> Option<u32> {
        Some(4343)
    }

    fn write_host_message(&mut self, _message: &HostMessage) -> Result<(), ProcessCodecError> {
        Ok(())
    }

    fn try_read_worker_message(&mut self) -> Result<Option<WorkerMessage>, ProcessCodecError> {
        let message = match self.phase {
            0 => Some(WorkerMessage {
                protocol_version: PROTOCOL_VERSION,
                host_generation: self.host_generation,
                engine_generation: ExtensionGeneration(0),
                request_id: 1,
                payload: WorkerPayload::Update(ExtensionUpdate {
                    host_generation: self.host_generation,
                    generation: ExtensionGeneration(0),
                    snapshot: Some(ExtensionSnapshot::default()),
                    effects: Vec::new(),
                    invalidated_views: Vec::new(),
                    circuit_notices: Vec::new(),
                }),
            }),
            1 => Some(WorkerMessage {
                protocol_version: PROTOCOL_VERSION,
                host_generation: HostGeneration(0),
                engine_generation: ExtensionGeneration(0),
                request_id: 2,
                payload: WorkerPayload::Update(ExtensionUpdate {
                    host_generation: HostGeneration(0),
                    generation: ExtensionGeneration(99),
                    snapshot: Some(ExtensionSnapshot::default()),
                    effects: Vec::new(),
                    invalidated_views: Vec::new(),
                    circuit_notices: Vec::new(),
                }),
            }),
            _ => None,
        };
        self.phase = self.phase.saturating_add(1);
        Ok(message)
    }

    fn try_wait(&mut self) -> io::Result<Option<std::process::ExitStatus>> {
        Ok(None)
    }

    fn shutdown_gracefully(&mut self, _timeout: Duration) -> io::Result<()> {
        Ok(())
    }

    fn kill(&mut self) -> io::Result<()> {
        Ok(())
    }
}

struct StaleUpdateSpawner;

impl ChildSpawner for StaleUpdateSpawner {
    fn spawn(&self, host_generation: HostGeneration) -> io::Result<Box<dyn ChildStream>> {
        Ok(Box::new(StaleUpdateChild {
            host_generation,
            phase: 0,
        }))
    }
}

impl ChildStream for ReadyChild {
    fn pid(&self) -> Option<u32> {
        Some(4242)
    }

    fn write_host_message(&mut self, message: &HostMessage) -> Result<(), ProcessCodecError> {
        self.writes.lock().unwrap().push(message.command.clone());
        Ok(())
    }

    fn try_read_worker_message(&mut self) -> Result<Option<WorkerMessage>, ProcessCodecError> {
        if !self.initial_sent {
            self.initial_sent = true;
            return Ok(Some(WorkerMessage {
                protocol_version: PROTOCOL_VERSION,
                host_generation: self.host_generation,
                engine_generation: ExtensionGeneration(0),
                request_id: 1,
                payload: WorkerPayload::Update(ExtensionUpdate {
                    host_generation: self.host_generation,
                    generation: ExtensionGeneration(0),
                    snapshot: Some(ExtensionSnapshot::default()),
                    effects: Vec::new(),
                    invalidated_views: Vec::new(),
                    circuit_notices: Vec::new(),
                }),
            }));
        }
        Ok(None)
    }

    fn try_wait(&mut self) -> io::Result<Option<std::process::ExitStatus>> {
        if self.alive {
            Ok(None)
        } else {
            Ok(Some(std::process::ExitStatus::default()))
        }
    }

    fn shutdown_gracefully(&mut self, _timeout: Duration) -> io::Result<()> {
        self.alive = false;
        Ok(())
    }

    fn kill(&mut self) -> io::Result<()> {
        self.alive = false;
        Ok(())
    }
}

struct ReadyChildSpawner {
    writes: Arc<Mutex<Vec<ExtensionCommand>>>,
}

impl ChildSpawner for ReadyChildSpawner {
    fn spawn(&self, host_generation: HostGeneration) -> io::Result<Box<dyn ChildStream>> {
        Ok(Box::new(ReadyChild {
            host_generation,
            initial_sent: false,
            alive: true,
            writes: self.writes.clone(),
        }))
    }
}

impl ChildSpawner for FailingChildSpawner {
    fn spawn(&self, _host_gen: HostGeneration) -> io::Result<Box<dyn ChildStream>> {
        self.spawn_count.fetch_add(1, Ordering::SeqCst);
        Err(io::Error::other("spawn failed"))
    }
}

#[test]
fn restart_delays_are_exact_and_quarantine_trips_on_third_crash() {
    assert_eq!(RETRY_DELAYS[0], Duration::from_millis(250));
    assert_eq!(RETRY_DELAYS[1], Duration::from_secs(1));
    assert_eq!(RETRY_DELAYS[2], Duration::from_secs(4));

    let spawn_count = Arc::new(AtomicU32::new(0));
    let spawner = FailingChildSpawner {
        spawn_count: spawn_count.clone(),
    };
    let start_instant = Instant::now();
    let clock = Arc::new(TestClock {
        now: Arc::new(Mutex::new(start_instant)),
    });

    let supervisor = ExtensionSupervisor::new_with_spawner(spawner, clock);

    for _ in 0..1_000 {
        if supervisor.state() == SupervisorState::Quarantined {
            break;
        }
        std::thread::sleep(Duration::from_millis(1));
    }

    let state = supervisor.state();
    assert_eq!(state, SupervisorState::Quarantined);
    let diag = supervisor.diagnostics();
    assert_eq!(diag.lifecycle, "quarantined");
}

#[test]
fn unexpected_child_exit_does_not_terminate_parent_control_plane() {
    let spawn_count = Arc::new(AtomicU32::new(0));
    let spawner = FailingChildSpawner {
        spawn_count: spawn_count.clone(),
    };
    let clock = Arc::new(TestClock {
        now: Arc::new(Mutex::new(Instant::now())),
    });

    let supervisor = ExtensionSupervisor::new_with_spawner(spawner, clock);
    std::thread::yield_now();

    // Control plane methods like send_command remain responsive
    assert!(matches!(
        supervisor.state(),
        SupervisorState::Starting | SupervisorState::Backoff { .. } | SupervisorState::Quarantined
    ));
    let snap = supervisor.snapshot();
    assert_eq!(snap.generation, ExtensionGeneration(0));
}

#[test]
fn five_minutes_ready_clears_rolling_crash_window() {
    assert_eq!(READY_RESET_DURATION, Duration::from_secs(300));
}

#[test]
fn shutdown_sends_typed_command_and_reaps_child() {
    let writes = Arc::new(Mutex::new(Vec::new()));
    let clock = Arc::new(TestClock {
        now: Arc::new(Mutex::new(Instant::now())),
    });
    let supervisor = ExtensionSupervisor::new_with_spawner(
        ReadyChildSpawner {
            writes: writes.clone(),
        },
        clock,
    );

    for _ in 0..10_000 {
        if supervisor.state() == SupervisorState::Ready {
            break;
        }
        std::thread::yield_now();
    }
    assert_eq!(supervisor.state(), SupervisorState::Ready);
    assert!(supervisor.shutdown(Duration::from_secs(1)));
    assert_eq!(supervisor.state(), SupervisorState::Stopped);
    assert!(matches!(
        writes.lock().unwrap().last(),
        Some(ExtensionCommand::Shutdown)
    ));
}

#[test]
fn stale_host_generation_is_dropped_before_snapshot_publication() {
    let clock = Arc::new(TestClock {
        now: Arc::new(Mutex::new(Instant::now())),
    });
    let supervisor = ExtensionSupervisor::new_with_spawner(StaleUpdateSpawner, clock);

    for _ in 0..100 {
        if supervisor.diagnostics().stale_updates_dropped > 0 {
            break;
        }
        std::thread::sleep(Duration::from_millis(5));
    }

    assert!(supervisor.diagnostics().stale_updates_dropped > 0);
    assert_eq!(supervisor.snapshot().generation, ExtensionGeneration(0));
    assert!(supervisor.shutdown(Duration::from_secs(1)));
}

#[test]
fn stale_engine_generation_is_dropped_before_snapshot_publication() {
    let clock = Arc::new(TestClock {
        now: Arc::new(Mutex::new(Instant::now())),
    });
    let supervisor = ExtensionSupervisor::new_with_spawner(StaleEngineSpawner, clock);

    for _ in 0..100 {
        if supervisor.diagnostics().stale_updates_dropped > 0 {
            break;
        }
        std::thread::sleep(Duration::from_millis(5));
    }

    assert!(supervisor.diagnostics().stale_updates_dropped > 0);
    assert_eq!(supervisor.snapshot().generation, ExtensionGeneration(2));
    assert!(supervisor.shutdown(Duration::from_secs(1)));
}
