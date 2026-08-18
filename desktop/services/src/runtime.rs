use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicU64, Ordering},
};

use shilpo_domain::MailboxError;
use tokio::sync::{mpsc, watch};

/// Capacity for bounded adapter command mailboxes under `MailboxPolicy::Lossless`.
pub const COMMAND_MAILBOX_CAPACITY: usize = 32;

struct TaskHandle {
    handle: tokio::task::JoinHandle<()>,
    cancellation: Cancellation,
}

impl Drop for TaskHandle {
    fn drop(&mut self) {
        self.cancellation.cancel();
        self.handle.abort();
    }
}

#[derive(Clone, Default)]
pub(crate) struct Cancellation(Arc<AtomicBool>);

impl Cancellation {
    fn cancel(&self) {
        self.0.store(true, Ordering::Release);
    }

    pub(crate) fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }
}

/// Context provided to state-only background tasks.
pub(crate) struct StateContext<State: Clone + Send + Sync + 'static> {
    tx: watch::Sender<State>,
    pub(crate) cancellation: Cancellation,
}

impl<State: Clone + Send + Sync + 'static> StateContext<State> {
    pub fn send_replace(&self, state: State) {
        let _ = self.tx.send_replace(state);
    }

    #[allow(dead_code)]
    pub fn update<F: FnOnce(&mut State)>(&self, f: F) {
        let mut current = self.tx.borrow().clone();
        f(&mut current);
        let _ = self.tx.send_replace(current);
    }

    pub fn get(&self) -> State {
        self.tx.borrow().clone()
    }
}

/// Context provided to command-enabled background tasks.
pub(crate) struct CommandContext<State: Clone + Send + Sync + 'static, Command: Send + 'static> {
    pub state: StateContext<State>,
    pub command_rx: mpsc::Receiver<Command>,
}

/// Crate-private shared state runtime managing channel broadcasting, clone ownership, and task lifecycle.
#[derive(Clone)]
pub(crate) struct StateRuntime<State: Clone + Send + Sync + 'static> {
    tx: watch::Sender<State>,
    _task: Option<Arc<TaskHandle>>,
}

impl<State: Clone + Send + Sync + 'static> StateRuntime<State> {
    pub fn new_offline(initial: State) -> Self {
        let (tx, _) = watch::channel(initial);
        Self { tx, _task: None }
    }

    pub fn spawn<F, Fut>(initial: State, offline: State, f: F) -> Self
    where
        F: FnOnce(StateContext<State>) -> Fut + Send + 'static,
        Fut: std::future::Future<Output = ()> + Send + 'static,
    {
        let (tx, _) = watch::channel(initial);
        let cancellation = Cancellation::default();
        let ctx = StateContext {
            tx: tx.clone(),
            cancellation: cancellation.clone(),
        };
        let task_tx = tx.clone();

        let task = if let Ok(handle) = tokio::runtime::Handle::try_current() {
            Some(Arc::new(TaskHandle {
                handle: handle.spawn(async move {
                    f(ctx).await;
                    let _ = task_tx.send_replace(offline);
                }),
                cancellation,
            }))
        } else {
            None
        };

        Self { tx, _task: task }
    }

    pub fn subscribe(&self) -> watch::Receiver<State> {
        self.tx.subscribe()
    }

    pub fn get(&self) -> State {
        self.tx.borrow().clone()
    }

    #[allow(dead_code)]
    pub fn update<F: FnOnce(&mut State)>(&self, f: F) {
        let mut current = self.get();
        f(&mut current);
        let _ = self.tx.send_replace(current);
    }

    pub fn send_replace(&self, state: State) {
        let _ = self.tx.send_replace(state);
    }

    #[allow(dead_code)]
    pub fn is_offline(&self) -> bool {
        self._task.is_none()
    }
}

/// Crate-private runtime wrapping `StateRuntime` with command channel support.
pub(crate) struct CommandRuntime<State: Clone + Send + Sync + 'static, Command: Send + 'static> {
    state: StateRuntime<State>,
    command_tx: Option<mpsc::Sender<Command>>,
    overloads: Arc<AtomicU64>,
}

impl<State: Clone + Send + Sync + 'static, Command: Send + 'static> Clone
    for CommandRuntime<State, Command>
{
    fn clone(&self) -> Self {
        Self {
            state: self.state.clone(),
            command_tx: self.command_tx.clone(),
            overloads: self.overloads.clone(),
        }
    }
}

impl<State: Clone + Send + Sync + 'static, Command: Send + 'static> CommandRuntime<State, Command> {
    pub fn new_offline(initial: State) -> Self {
        Self {
            state: StateRuntime::new_offline(initial),
            command_tx: None,
            overloads: Arc::new(AtomicU64::new(0)),
        }
    }

    pub fn spawn<F, Fut>(initial: State, offline: State, f: F) -> Self
    where
        F: FnOnce(CommandContext<State, Command>) -> Fut + Send + 'static,
        Fut: std::future::Future<Output = ()> + Send + 'static,
    {
        // Bounded adapter command mailbox: MailboxPolicy::Lossless, capacity 32.
        // Carries distinct user intents (set wifi, connect VPN, set profile, media transport).
        // Silently dropping one loses a user action.
        let (command_tx, command_rx) = mpsc::channel(COMMAND_MAILBOX_CAPACITY);
        let state = StateRuntime::spawn(initial, offline, move |state_ctx| {
            let cmd_ctx = CommandContext {
                state: state_ctx,
                command_rx,
            };
            f(cmd_ctx)
        });

        let command_tx = (!state.is_offline()).then_some(command_tx);
        let overloads = Arc::new(AtomicU64::new(0));
        Self {
            state,
            command_tx,
            overloads,
        }
    }

    pub fn subscribe(&self) -> watch::Receiver<State> {
        self.state.subscribe()
    }

    pub fn get(&self) -> State {
        self.state.get()
    }

    pub fn send_command(&self, command: Command) -> Result<(), MailboxError> {
        if let Some(tx) = &self.command_tx {
            match tx.try_send(command) {
                Ok(()) => Ok(()),
                Err(mpsc::error::TrySendError::Full(_)) => {
                    self.overloads.fetch_add(1, Ordering::SeqCst);
                    tracing::warn!(
                        site = "CommandRuntime",
                        policy = "Lossless",
                        capacity = COMMAND_MAILBOX_CAPACITY,
                        "adapter command mailbox full; command rejected"
                    );
                    Err(MailboxError::Overloaded)
                }
                Err(mpsc::error::TrySendError::Closed(_)) => {
                    tracing::warn!(site = "CommandRuntime", "adapter command mailbox closed");
                    Err(MailboxError::Unavailable)
                }
            }
        } else {
            Err(MailboxError::Unavailable)
        }
    }

    #[allow(dead_code)]
    pub fn overloads(&self) -> u64 {
        self.overloads.load(Ordering::SeqCst)
    }

    #[allow(dead_code)]
    pub fn send_replace(&self, state: State) {
        self.state.send_replace(state);
    }

    #[allow(dead_code)]
    pub fn is_offline(&self) -> bool {
        self.state.is_offline()
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    #[tokio::test]
    async fn test_state_runtime_offline() {
        let runtime = StateRuntime::new_offline(10);
        assert_eq!(runtime.get(), 10);
        assert!(runtime.is_offline());
        let mut rx = runtime.subscribe();
        assert_eq!(*rx.borrow(), 10);

        runtime.send_replace(20);
        assert_eq!(runtime.get(), 20);
        assert!(rx.changed().await.is_ok());
        assert_eq!(*rx.borrow(), 20);
    }

    #[tokio::test]
    async fn test_command_runtime_dispatch() {
        let runtime = CommandRuntime::spawn(0, -1, |mut ctx| async move {
            while let Some(cmd) = ctx.command_rx.recv().await {
                match cmd {
                    1 => ctx.state.send_replace(100),
                    2 => ctx.state.update(|val| *val += 5),
                    _ => {}
                }
            }
        });

        let mut rx = runtime.subscribe();
        assert_eq!(runtime.send_command(1), Ok(()));
        assert!(rx.changed().await.is_ok());
        assert_eq!(runtime.get(), 100);

        assert_eq!(runtime.send_command(2), Ok(()));
        assert!(rx.changed().await.is_ok());
        assert_eq!(runtime.get(), 105);
    }

    #[tokio::test]
    async fn test_command_runtime_lossless_overload_and_delivery() {
        let (release_tx, release_rx) = tokio::sync::oneshot::channel::<()>();
        let (delivered_tx, mut delivered_rx) = mpsc::channel(COMMAND_MAILBOX_CAPACITY);

        let runtime = CommandRuntime::spawn(0, -1, move |mut ctx| async move {
            let _ = release_rx.await;
            while let Some(cmd) = ctx.command_rx.recv().await {
                let _ = delivered_tx.send(cmd).await;
            }
        });

        // 1. Send capacity commands: all return Ok(())
        for i in 0..COMMAND_MAILBOX_CAPACITY {
            assert_eq!(runtime.send_command(i), Ok(()));
        }
        assert_eq!(runtime.overloads(), 0);

        // 2. Capacity+1 command returns Err(MailboxError::Overloaded) and increments overloads
        assert_eq!(runtime.send_command(999), Err(MailboxError::Overloaded));
        assert_eq!(runtime.overloads(), 1);

        // 3. Release the loop and verify all capacity accepted commands arrive in order
        release_tx.send(()).unwrap();
        for i in 0..COMMAND_MAILBOX_CAPACITY {
            assert_eq!(delivered_rx.recv().await, Some(i));
        }
    }

    #[tokio::test]
    async fn test_command_runtime_offline_returns_unavailable() {
        let runtime: CommandRuntime<i32, i32> = CommandRuntime::new_offline(0);
        let err = runtime.send_command(1).unwrap_err();
        assert_eq!(err, MailboxError::Unavailable);
        assert_ne!(err, MailboxError::Overloaded);
    }

    #[tokio::test]
    async fn test_shared_clone_ownership_and_cancellation() {
        let (drop_tx, mut drop_rx) = mpsc::channel::<()>(1);

        let runtime1 = StateRuntime::spawn(0, 0, move |_ctx| async move {
            let _sentinel = drop_tx;
            tokio::time::sleep(Duration::from_secs(60)).await;
        });

        let runtime2 = runtime1.clone();
        drop(runtime1);

        // Task should still be running because runtime2 exists
        tokio::task::yield_now().await;
        assert!(drop_rx.try_recv().is_err());

        // Dropping last runtime clone should abort task
        drop(runtime2);
        tokio::task::yield_now().await;
        assert!(drop_rx.recv().await.is_none());
    }

    #[tokio::test]
    async fn test_backend_exit_publishes_offline_state() {
        let runtime = StateRuntime::spawn(1, 0, |_ctx| async {});
        let mut rx = runtime.subscribe();
        assert!(rx.changed().await.is_ok());
        assert_eq!(*rx.borrow(), 0);
    }
}
