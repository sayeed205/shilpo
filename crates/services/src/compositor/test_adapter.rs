use super::{CompositorAdapter, CompositorCommand, CompositorSnapshot};
use std::sync::{Arc, Mutex};
use tokio::sync::watch;

/// In-memory compositor adapter implementation for testing.
pub struct TestCompositorAdapter {
    tx: watch::Sender<Arc<CompositorSnapshot>>,
    rx: watch::Receiver<Arc<CompositorSnapshot>>,
    executed_commands: Arc<Mutex<Vec<CompositorCommand>>>,
}

impl TestCompositorAdapter {
    pub fn new(initial: CompositorSnapshot) -> Self {
        let (tx, rx) = watch::channel(Arc::new(initial));
        Self {
            tx,
            rx,
            executed_commands: Arc::new(Mutex::new(Vec::new())),
        }
    }

    pub fn new_default() -> Self {
        Self::new(CompositorSnapshot::default())
    }

    pub fn update(&self, snapshot: CompositorSnapshot) {
        let _ = self.tx.send(Arc::new(snapshot));
    }

    pub fn executed_commands(&self) -> Vec<CompositorCommand> {
        self.executed_commands.lock().unwrap().clone()
    }
}

impl CompositorAdapter for TestCompositorAdapter {
    fn current(&self) -> Arc<CompositorSnapshot> {
        self.rx.borrow().clone()
    }

    fn subscribe(&self) -> watch::Receiver<Arc<CompositorSnapshot>> {
        self.rx.clone()
    }

    fn execute(&self, command: CompositorCommand) -> anyhow::Result<()> {
        let snapshot = self.current();
        if !snapshot.connection.is_ready() {
            anyhow::bail!(
                "Compositor is unavailable: state is {:?}",
                snapshot.connection
            );
        }
        self.executed_commands.lock().unwrap().push(command);
        Ok(())
    }
}
