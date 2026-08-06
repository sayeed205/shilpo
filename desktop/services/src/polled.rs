use std::time::Duration;
use tokio::sync::watch;

/// Configuration options for exponential backoff on polling errors.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BackoffConfig {
    pub initial_delay: Duration,
    pub max_delay: Duration,
    pub factor: u32,
}

impl Default for BackoffConfig {
    fn default() -> Self {
        Self {
            initial_delay: Duration::from_secs(3),
            max_delay: Duration::from_secs(60),
            factor: 2,
        }
    }
}

/// Generic polling harness owning state cache, watch channel, retry backoff, and task lifecycle.
pub struct PolledService<T: Clone + Send + Sync + 'static> {
    tx: watch::Sender<T>,
    _task: Option<tokio::task::JoinHandle<()>>,
}

impl<T: Clone + Send + Sync + 'static> Drop for PolledService<T> {
    fn drop(&mut self) {
        if let Some(task) = self._task.take() {
            task.abort();
        }
    }
}

impl<T: Clone + Send + Sync + 'static> PolledService<T> {
    /// Creates a new `PolledService` that periodically executes `sample` and broadcasts updates.
    pub fn new<F, E>(
        initial_state: T,
        poll_interval: Duration,
        backoff: Option<BackoffConfig>,
        mut sample: F,
    ) -> Self
    where
        F: FnMut(&T) -> Result<T, E> + Send + 'static,
        E: std::fmt::Display,
    {
        let (tx, _rx) = watch::channel(initial_state);
        let tx_clone = tx.clone();

        let task = if let Ok(handle) = tokio::runtime::Handle::try_current() {
            Some(handle.spawn(async move {
                let mut current_interval = poll_interval;
                loop {
                    let current_state = tx_clone.borrow().clone();
                    match sample(&current_state) {
                        Ok(new_state) => {
                            let _ = tx_clone.send_replace(new_state);
                            current_interval = poll_interval;
                        }
                        Err(err) => {
                            tracing::debug!("PolledService sample failed: {}", err);
                            if let Some(cfg) = backoff {
                                current_interval =
                                    (current_interval * cfg.factor).min(cfg.max_delay);
                            }
                        }
                    }
                    tokio::time::sleep(current_interval).await;
                }
            }))
        } else {
            None
        };

        Self { tx, _task: task }
    }

    /// Creates an offline `PolledService` without spawning a background polling task.
    pub fn new_offline(initial_state: T) -> Self {
        let (tx, _rx) = watch::channel(initial_state);
        Self { tx, _task: None }
    }

    /// Returns a watch receiver to subscribe to state updates.
    pub fn subscribe(&self) -> watch::Receiver<T> {
        self.tx.subscribe()
    }

    /// Returns a clone of the current state snapshot.
    pub fn get(&self) -> T {
        self.tx.borrow().clone()
    }

    /// Updates the stored state in-place and notifies subscribers.
    pub fn update<F>(&self, f: F)
    where
        F: FnOnce(&mut T),
    {
        let mut current = self.get();
        f(&mut current);
        let _ = self.tx.send_replace(current);
    }

    /// Replaces the stored state with a new value and notifies subscribers.
    pub fn send_replace(&self, state: T) {
        let _ = self.tx.send_replace(state);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    #[tokio::test]
    async fn test_polled_service_offline() {
        let service = PolledService::new_offline(42);
        assert_eq!(service.get(), 42);
        let mut rx = service.subscribe();
        assert_eq!(*rx.borrow(), 42);

        service.send_replace(100);
        assert_eq!(service.get(), 100);
        assert!(rx.changed().await.is_ok());
        assert_eq!(*rx.borrow(), 100);
    }

    #[tokio::test]
    async fn test_polled_service_updates() {
        let counter = Arc::new(AtomicUsize::new(0));
        let counter_clone = counter.clone();

        let service = PolledService::new(
            0,
            Duration::from_millis(20),
            None,
            move |_curr: &usize| -> Result<usize, String> {
                let val = counter_clone.fetch_add(1, Ordering::SeqCst) + 1;
                Ok(val)
            },
        );

        let mut rx = service.subscribe();
        rx.changed().await.unwrap();
        assert!(*rx.borrow() > 0);
        drop(service);
    }

    #[tokio::test]
    async fn test_polled_service_error_backoff() {
        let fail_counter = Arc::new(AtomicUsize::new(0));
        let fail_counter_clone = fail_counter.clone();

        let backoff = BackoffConfig {
            initial_delay: Duration::from_millis(10),
            max_delay: Duration::from_millis(50),
            factor: 2,
        };

        let service = PolledService::new(
            0,
            Duration::from_millis(10),
            Some(backoff),
            move |_curr: &usize| -> Result<usize, &'static str> {
                fail_counter_clone.fetch_add(1, Ordering::SeqCst);
                Err("simulated failure")
            },
        );

        tokio::time::sleep(Duration::from_millis(80)).await;
        assert!(fail_counter.load(Ordering::SeqCst) >= 2);
        assert_eq!(service.get(), 0);
        drop(service);
    }

    #[tokio::test]
    async fn test_polled_service_cancellation() {
        let (tx, mut rx) = tokio::sync::mpsc::channel::<()>(1);
        let (watch_tx, _) = watch::channel(0);

        let task = tokio::spawn(async move {
            let _sentinel = tx;
            tokio::time::sleep(Duration::from_secs(10)).await;
        });

        let service = PolledService {
            tx: watch_tx,
            _task: Some(task),
        };

        tokio::task::yield_now().await;
        drop(service);
        tokio::task::yield_now().await;

        assert!(
            rx.recv().await.is_none(),
            "Task sentinel should be dropped when PolledService is dropped"
        );
    }
}
