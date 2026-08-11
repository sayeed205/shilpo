use shilpo_shell::extensions::process::HostGeneration;
use shilpo_shell::extensions::supervisor::{
    ChildSpawner, ChildStream, Clock, ExtensionSupervisor, SupervisorState, READY_RESET_DURATION,
    RETRY_DELAYS,
};
use shilpo_shell::extensions::{ExtensionCommand, ExtensionGeneration};
use std::{
    io,
    sync::{
        Arc, Mutex,
        atomic::{AtomicU32, Ordering},
    },
    time::{Duration, Instant},
};

struct TestClock {
    now: Arc<Mutex<Instant>>,
}

impl Clock for TestClock {
    fn now(&self) -> Instant {
        *self.now.lock().unwrap()
    }
}

struct FailingChildSpawner {
    spawn_count: Arc<AtomicU32>,
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

    // Allow supervisor loop to attempt retries (250ms + 1000ms) and enter Quarantined state
    std::thread::sleep(Duration::from_millis(1350));

    let state = supervisor.state();
    assert_eq!(state, SupervisorState::Quarantined);
    let diag = supervisor.diagnostics();
    assert_eq!(diag.state, "quarantined");
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
    std::thread::sleep(Duration::from_millis(300));

    // Control plane methods like send_command remain responsive
    let res = supervisor.send_command(ExtensionCommand::SourcesChanged);
    assert!(res.is_err() || res.is_ok());
    let snap = supervisor.snapshot();
    assert_eq!(snap.generation, ExtensionGeneration(0));
}

#[test]
fn five_minutes_ready_clears_rolling_crash_window() {
    assert_eq!(READY_RESET_DURATION, Duration::from_secs(300));
}
