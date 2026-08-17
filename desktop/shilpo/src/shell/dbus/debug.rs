//! org.shilpo.Debug D-Bus service implementation.

use shilpo_observability::{FilterError, LogFilterController};
use tokio::sync::mpsc;

use super::server::ShellCommand;

/// D-Bus interface implementation for `org.shilpo.Debug`.
#[derive(Clone)]
pub struct DebugDbusService {
    filter_controller: Option<LogFilterController>,
    mailbox_tx: mpsc::Sender<ShellCommand>,
}

impl DebugDbusService {
    pub fn new(
        filter_controller: Option<LogFilterController>,
        mailbox_tx: mpsc::Sender<ShellCommand>,
    ) -> Self {
        Self {
            filter_controller,
            mailbox_tx,
        }
    }
}

#[zbus::interface(name = "org.shilpo.Debug")]
impl DebugDbusService {
    async fn set_log_filter(&self, filter: String) -> zbus::fdo::Result<()> {
        let _span = tracing::info_span!(
            target: "shilpo_profile",
            "dbus_call",
            destination = "org.shilpo.Debug",
            operation = "set_log_filter",
            outcome = tracing::field::Empty
        );
        let _enter = _span.enter();

        let controller = match self.filter_controller.as_ref() {
            Some(c) => c,
            None => {
                tracing::Span::current().record("outcome", "failed");
                return Err(zbus::fdo::Error::Failed(
                    "filter controller unavailable".into(),
                ));
            }
        };

        match controller.set_filter(&filter) {
            Ok(()) => {
                tracing::Span::current().record("outcome", "success");
                Ok(())
            }
            Err(FilterError::EmptyFilter) => {
                tracing::Span::current().record("outcome", "invalid_args");
                Err(zbus::fdo::Error::InvalidArgs(
                    "filter directive cannot be empty or whitespace".into(),
                ))
            }
            Err(FilterError::InvalidFilter(reason)) => {
                tracing::Span::current().record("outcome", "invalid_args");
                Err(zbus::fdo::Error::InvalidArgs(reason))
            }
            Err(FilterError::ReloadFailed(reason)) => {
                tracing::Span::current().record("outcome", "failed");
                Err(zbus::fdo::Error::Failed(reason))
            }
        }
    }

    async fn get_log_filter(&self) -> zbus::fdo::Result<String> {
        let _span = tracing::info_span!(
            target: "shilpo_profile",
            "dbus_call",
            destination = "org.shilpo.Debug",
            operation = "get_log_filter",
            outcome = tracing::field::Empty
        );
        let _enter = _span.enter();

        let controller = match self.filter_controller.as_ref() {
            Some(c) => c,
            None => {
                tracing::Span::current().record("outcome", "failed");
                return Err(zbus::fdo::Error::Failed(
                    "filter controller unavailable".into(),
                ));
            }
        };

        let current = controller.current_filter();
        tracing::Span::current().record("outcome", "success");
        Ok(current)
    }

    async fn emit_test_notification(&self, title: String, body: String) -> zbus::fdo::Result<()> {
        let _span = tracing::info_span!(
            target: "shilpo_profile",
            "dbus_call",
            destination = "org.shilpo.Debug",
            operation = "emit_test_notification",
            outcome = tracing::field::Empty
        );
        let _enter = _span.enter();

        if title.trim().is_empty() {
            tracing::Span::current().record("outcome", "invalid_args");
            return Err(zbus::fdo::Error::InvalidArgs(
                "notification title cannot be empty or whitespace".into(),
            ));
        }

        if title.len() > 256 {
            tracing::Span::current().record("outcome", "invalid_args");
            return Err(zbus::fdo::Error::InvalidArgs(
                "notification title exceeds 256 bytes limit".into(),
            ));
        }

        if body.len() > 4096 {
            tracing::Span::current().record("outcome", "invalid_args");
            return Err(zbus::fdo::Error::InvalidArgs(
                "notification body exceeds 4096 bytes limit".into(),
            ));
        }

        let result = match self
            .mailbox_tx
            .try_send(ShellCommand::EmitTestNotification { title, body })
        {
            Ok(()) => Ok(()),
            Err(mpsc::error::TrySendError::Full(_)) => Err(zbus::fdo::Error::LimitsExceeded(
                "command mailbox is full".into(),
            )),
            Err(mpsc::error::TrySendError::Closed(_)) => {
                Err(zbus::fdo::Error::Failed("shell daemon is stopping".into()))
            }
        };

        tracing::Span::current().record(
            "outcome",
            if result.is_ok() { "accepted" } else { "failed" },
        );
        result
    }

    async fn reset_notification_quarantine(&self) -> zbus::fdo::Result<()> {
        let _span = tracing::info_span!(
            target: "shilpo_profile",
            "dbus_call",
            destination = "org.shilpo.Debug",
            operation = "reset_notification_quarantine",
            outcome = tracing::field::Empty
        );
        let _enter = _span.enter();

        let result = match self
            .mailbox_tx
            .try_send(ShellCommand::ResetNotificationQuarantine)
        {
            Ok(()) => Ok(()),
            Err(mpsc::error::TrySendError::Full(_)) => Err(zbus::fdo::Error::LimitsExceeded(
                "command mailbox is full".into(),
            )),
            Err(mpsc::error::TrySendError::Closed(_)) => {
                Err(zbus::fdo::Error::Failed("shell daemon is stopping".into()))
            }
        };

        tracing::Span::current().record(
            "outcome",
            if result.is_ok() { "accepted" } else { "failed" },
        );
        result
    }
}
