use super::{
    BrokerOptions, CompositorAdapter, CompositorCommand, CompositorCommandBroker,
    CompositorSnapshot,
};
use std::sync::{Arc, Mutex};
use tokio::sync::watch;

/// In-memory compositor adapter implementation for testing.
pub struct TestCompositorAdapter {
    tx: watch::Sender<Arc<CompositorSnapshot>>,
    rx: watch::Receiver<Arc<CompositorSnapshot>>,
    executed_commands: Arc<Mutex<Vec<CompositorCommand>>>,
    broker: Arc<CompositorCommandBroker>,
}

impl TestCompositorAdapter {
    pub fn new(initial: CompositorSnapshot) -> Self {
        let (tx, rx) = watch::channel(Arc::new(initial.clone()));
        let executed_commands = Arc::new(Mutex::new(Vec::new()));

        let exec_cmds = executed_commands.clone();
        let executor: super::broker::CommandExecutorFn =
            Box::new(move |cmd, _timeout, _cancel, _register| {
                exec_cmds.lock().unwrap().push(cmd.clone());
                Ok(())
            });

        let broker = CompositorCommandBroker::new(BrokerOptions::default(), executor);
        broker.update_connection(initial.connection, initial.capabilities);

        Self {
            tx,
            rx,
            executed_commands,
            broker,
        }
    }

    pub fn new_default() -> Self {
        Self::new(CompositorSnapshot::default())
    }

    pub fn update(&self, snapshot: CompositorSnapshot) {
        self.broker
            .update_connection(snapshot.connection.clone(), snapshot.capabilities.clone());
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

    fn command_broker(&self) -> Arc<CompositorCommandBroker> {
        self.broker.clone()
    }
}
