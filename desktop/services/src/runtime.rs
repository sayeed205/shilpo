use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use tokio::sync::{mpsc, watch};

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
    pub command_rx: mpsc::UnboundedReceiver<Command>,
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
    command_tx: Option<mpsc::UnboundedSender<Command>>,
}

impl<State: Clone + Send + Sync + 'static, Command: Send + 'static> Clone
    for CommandRuntime<State, Command>
{
    fn clone(&self) -> Self {
        Self {
            state: self.state.clone(),
            command_tx: self.command_tx.clone(),
        }
    }
}

impl<State: Clone + Send + Sync + 'static, Command: Send + 'static> CommandRuntime<State, Command> {
    pub fn new_offline(initial: State) -> Self {
        Self {
            state: StateRuntime::new_offline(initial),
            command_tx: None,
        }
    }

    pub fn spawn<F, Fut>(initial: State, offline: State, f: F) -> Self
    where
        F: FnOnce(CommandContext<State, Command>) -> Fut + Send + 'static,
        Fut: std::future::Future<Output = ()> + Send + 'static,
    {
        let (command_tx, command_rx) = mpsc::unbounded_channel();
        let state = StateRuntime::spawn(initial, offline, move |state_ctx| {
            let cmd_ctx = CommandContext {
                state: state_ctx,
                command_rx,
            };
            f(cmd_ctx)
        });

        let command_tx = (!state.is_offline()).then_some(command_tx);
        Self { state, command_tx }
    }

    pub fn subscribe(&self) -> watch::Receiver<State> {
        self.state.subscribe()
    }

    pub fn get(&self) -> State {
        self.state.get()
    }

    pub fn send_command(&self, command: Command) -> bool {
        if let Some(tx) = &self.command_tx {
            tx.send(command).is_ok()
        } else {
            false
        }
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
    use super::*;
    use std::time::Duration;

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
        assert!(runtime.send_command(1));
        assert!(rx.changed().await.is_ok());
        assert_eq!(runtime.get(), 100);

        assert!(runtime.send_command(2));
        assert!(rx.changed().await.is_ok());
        assert_eq!(runtime.get(), 105);
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
