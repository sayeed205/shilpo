//! D-Bus adapter for shell commands and status queries over `org.shilpo.Shell`.

use crate::shell::dbus::{CommandResult, ShellProxy, ShellStatus, ShellTelemetry};
use std::sync::{Mutex, OnceLock};
use zbus::Connection;

pub struct IpcAdapter;

impl Default for IpcAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl IpcAdapter {
    pub fn new() -> Self {
        Self
    }

    fn get_proxy() -> Result<(Connection, ShellProxy<'static>), (i32, String)> {
        static SESSION: OnceLock<Mutex<Option<Connection>>> = OnceLock::new();
        let session = SESSION.get_or_init(|| Mutex::new(None));
        futures_lite::future::block_on(async {
            let conn = session.lock().unwrap().clone();
            let conn = if let Some(conn) = conn {
                conn
            } else {
                let conn = Connection::session().await.map_err(map_dbus_error)?;
                *session.lock().unwrap() = Some(conn.clone());
                conn
            };
            let proxy = ShellProxy::builder(&conn)
                .build()
                .await
                .map_err(map_dbus_error)?;
            Ok((conn, proxy))
        })
    }

    pub fn status(&self) -> Result<ShellStatus, (i32, String)> {
        let (_conn, proxy) = Self::get_proxy()?;
        futures_lite::future::block_on(async { proxy.get_status().await.map_err(map_dbus_error) })
    }

    pub fn telemetry(&self) -> Result<ShellTelemetry, (i32, String)> {
        let (_conn, proxy) = Self::get_proxy()?;
        futures_lite::future::block_on(async {
            proxy.get_telemetry().await.map_err(map_dbus_error)
        })
    }

    pub fn overview_show(&self) -> Result<(), (i32, String)> {
        let (_conn, proxy) = Self::get_proxy()?;
        futures_lite::future::block_on(async {
            proxy.show_overview().await.map_err(map_dbus_error)
        })
    }

    pub fn overview_hide(&self) -> Result<(), (i32, String)> {
        let (_conn, proxy) = Self::get_proxy()?;
        futures_lite::future::block_on(async {
            proxy.hide_overview().await.map_err(map_dbus_error)
        })
    }

    pub fn overview_toggle(&self) -> Result<(), (i32, String)> {
        let (_conn, proxy) = Self::get_proxy()?;
        futures_lite::future::block_on(async {
            proxy.toggle_overview().await.map_err(map_dbus_error)
        })
    }

    pub fn bar_show(&self) -> Result<(), (i32, String)> {
        let (_conn, proxy) = Self::get_proxy()?;
        futures_lite::future::block_on(async { proxy.show_bar().await.map_err(map_dbus_error) })
    }

    pub fn bar_hide(&self) -> Result<(), (i32, String)> {
        let (_conn, proxy) = Self::get_proxy()?;
        futures_lite::future::block_on(async { proxy.hide_bar().await.map_err(map_dbus_error) })
    }

    pub fn bar_toggle(&self) -> Result<(), (i32, String)> {
        let (_conn, proxy) = Self::get_proxy()?;
        futures_lite::future::block_on(async { proxy.toggle_bar().await.map_err(map_dbus_error) })
    }

    pub fn workspace_focus(&self, id: u64) -> Result<(), (i32, String)> {
        let (_conn, proxy) = Self::get_proxy()?;
        futures_lite::future::block_on(async {
            let res = proxy.focus_workspace(id).await.map_err(map_dbus_error)?;
            map_command_result(res)
        })
    }

    pub fn workspace_create(&self) -> Result<(), (i32, String)> {
        let (_conn, proxy) = Self::get_proxy()?;
        futures_lite::future::block_on(async {
            let res = proxy.create_workspace().await.map_err(map_dbus_error)?;
            map_command_result(res)
        })
    }

    pub fn window_focus(&self, id: u64) -> Result<(), (i32, String)> {
        let (_conn, proxy) = Self::get_proxy()?;
        futures_lite::future::block_on(async {
            let res = proxy.focus_window(id).await.map_err(map_dbus_error)?;
            map_command_result(res)
        })
    }

    pub fn window_focus_previous(&self) -> Result<(), (i32, String)> {
        let (_conn, proxy) = Self::get_proxy()?;
        futures_lite::future::block_on(async {
            let res = proxy
                .focus_previous_window()
                .await
                .map_err(map_dbus_error)?;
            map_command_result(res)
        })
    }

    pub fn window_move(&self, window_id: u64, workspace_id: u64) -> Result<(), (i32, String)> {
        let (_conn, proxy) = Self::get_proxy()?;
        futures_lite::future::block_on(async {
            let res = proxy
                .move_window_to_workspace(window_id, workspace_id)
                .await
                .map_err(map_dbus_error)?;
            map_command_result(res)
        })
    }

    pub fn config_reload(&self) -> Result<(), (i32, String)> {
        let (_conn, proxy) = Self::get_proxy()?;
        futures_lite::future::block_on(async {
            proxy.reload_config().await.map_err(map_dbus_error)
        })
    }

    pub fn set_brightness(&self, percentage: u8) -> Result<(), (i32, String)> {
        let (_conn, proxy) = Self::get_proxy()?;
        futures_lite::future::block_on(async {
            proxy
                .set_brightness(percentage)
                .await
                .map_err(map_dbus_error)
        })
    }

    pub fn set_display_brightness(
        &self,
        display_id: String,
        percentage: u8,
    ) -> Result<(), (i32, String)> {
        let (_conn, proxy) = Self::get_proxy()?;
        futures_lite::future::block_on(async {
            proxy
                .set_display_brightness(display_id, percentage)
                .await
                .map_err(map_dbus_error)
        })
    }

    pub fn capture(
        &self,
        intent: shilpo_services::capture::CaptureIntent,
    ) -> Result<(), (i32, String)> {
        let (_conn, proxy) = Self::get_proxy()?;
        let intent_str = match intent {
            shilpo_services::capture::CaptureIntent::Clipboard => "clipboard",
            shilpo_services::capture::CaptureIntent::Annotation => "annotation",
            shilpo_services::capture::CaptureIntent::Ocr => "ocr",
            shilpo_services::capture::CaptureIntent::Menu => "menu",
        };
        futures_lite::future::block_on(async {
            proxy
                .capture(intent_str.to_string())
                .await
                .map_err(map_dbus_error)
        })
    }

    pub fn action_invoke(
        &self,
        action_id: String,
        payload: Option<String>,
    ) -> Result<(), (i32, String)> {
        let (_conn, proxy) = Self::get_proxy()?;
        futures_lite::future::block_on(async {
            proxy
                .invoke_action(action_id, payload)
                .await
                .map_err(map_dbus_error)
        })
    }
}

fn map_command_result(res: CommandResult) -> Result<(), (i32, String)> {
    if res.is_applied() {
        Ok(())
    } else {
        match res.outcome.as_str() {
            "rejected" => Err((1, format!("command rejected: {}", res.reason))),
            "timed_out" => Err((4, "command timed out".to_string())),
            "cancelled" => Err((1, format!("command cancelled: {}", res.reason))),
            _ => Err((1, format!("command failed: {}", res.outcome))),
        }
    }
}

pub fn map_dbus_error(err: zbus::Error) -> (i32, String) {
    match &err {
        zbus::Error::FDO(fdo_err) => match &**fdo_err {
            zbus::fdo::Error::UnknownMethod(msg)
            | zbus::fdo::Error::ServiceUnknown(msg)
            | zbus::fdo::Error::NameHasNoOwner(msg) => {
                (3, format!("shell daemon unavailable: {msg}"))
            }
            zbus::fdo::Error::InvalidArgs(msg) => (2, format!("invalid arguments: {msg}")),
            zbus::fdo::Error::LimitsExceeded(msg) => (1, format!("limits exceeded: {msg}")),
            zbus::fdo::Error::Failed(msg) => (1, format!("operation failed: {msg}")),
            _ => (1, format!("{fdo_err}")),
        },
        _ => (3, format!("shell daemon unavailable: {err}")),
    }
}
