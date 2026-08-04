use super::{CaptureBackend, CaptureError, CapturedFrame, OutputInfo};
use std::process::Command;
use std::time::{Duration, SystemTime};
use wayland_client::Connection;

/// Production Wayland capture backend using `ext-image-copy-capture-v1`
/// (when supported) and `zwlr_screencopy_manager_v3` (for Niri/wlroots compatibility).
pub struct WaylandCaptureBackend {
    conn: Connection,
}

impl WaylandCaptureBackend {
    pub fn connect() -> Result<Self, CaptureError> {
        let conn = Connection::connect_to_env()
            .map_err(|e| CaptureError::Unavailable(format!("Wayland connection failed: {e}")))?;
        Ok(Self { conn })
    }
}

/// Capture a compositor-approved frame through the XDG Screenshot portal.
///
/// The portal owns the compositor interaction and returns a temporary PNG;
/// Shilpo owns everything after that point (selection, clipboard, annotation,
/// and persistence). This gives us a real Wayland frame without shelling out
/// to grim/slurp or relying on fabricated output geometry.
pub async fn capture_via_portal() -> Result<CapturedFrame, CaptureError> {
    use ashpd::desktop::screenshot::{AvailableTargets, Screenshot, ScreenshotProxy};

    // Portal implementations differ across compositors. Niri's portal may
    // expose only an interactive area target, while GNOME exposes Screen.
    // Negotiate instead of assuming target bit 1 is available.
    let proxy = ScreenshotProxy::new().await.map_err(|error| {
        CaptureError::Unavailable(format!("screenshot portal unavailable: {error}"))
    })?;
    let targets = proxy
        .available_targets()
        .await
        .unwrap_or_else(|_| AvailableTargets::Screen | AvailableTargets::Area);
    let request_result = if targets.contains(AvailableTargets::Screen) {
        Screenshot::request()
            .interactive(false)
            .modal(true)
            .target(AvailableTargets::Screen)
            .send()
            .await
    } else if targets.contains(AvailableTargets::Area) {
        Screenshot::request()
            .interactive(true)
            .modal(true)
            .target(AvailableTargets::Area)
            .send()
            .await
    } else {
        Screenshot::request()
            .interactive(true)
            .modal(true)
            .send()
            .await
    };
    let request = match request_result {
        Ok(request) => request,
        Err(first_error) => Screenshot::request()
            .interactive(true)
            .modal(true)
            .send()
            .await
            .map_err(|fallback_error| {
                CaptureError::Unavailable(format!(
                    "screenshot portal request failed ({first_error}); fallback failed: {fallback_error}"
                ))
            })?,
    };
    let response = request.response().map_err(|error| {
        CaptureError::Rejected(format!("screenshot portal rejected request: {error}"))
    })?;
    let uri = response.uri().as_str();
    let path = uri
        .strip_prefix("file://")
        .ok_or_else(|| CaptureError::Protocol(format!("unsupported screenshot URI: {uri}")))?;
    let image = image::open(path)
        .map_err(|error| {
            CaptureError::Buffer(format!("could not decode portal screenshot: {error}"))
        })?
        .to_rgba8();
    let _ = std::fs::remove_file(path);
    Ok(CapturedFrame {
        image,
        transform: super::OutputTransform::Normal,
        protocol: "xdg-desktop-portal-screenshot",
    })
}

/// Capture an output through `wlr-screencopy` without triggering the desktop's
/// user-facing screenshot workflow (clipboard writes, saved files, or
/// notifications).
pub fn capture_via_grim(output_name: Option<&str>) -> Result<CapturedFrame, CaptureError> {
    let mut command = Command::new("grim");
    command.args(grim_capture_args(output_name));

    let output = command
        .output()
        .map_err(|error| CaptureError::Unavailable(format!("could not invoke grim: {error}")))?;
    if !output.status.success() {
        return Err(CaptureError::Rejected(format!(
            "grim exited with {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }
    let image = image::load_from_memory_with_format(&output.stdout, image::ImageFormat::Png)
        .map_err(|error| CaptureError::Buffer(format!("could not decode grim frame: {error}")))?
        .to_rgba8();
    Ok(CapturedFrame {
        image,
        transform: super::OutputTransform::Normal,
        protocol: "wlr-screencopy-v3",
    })
}

fn grim_capture_args(output_name: Option<&str>) -> Vec<String> {
    let mut args = vec!["-t".into(), "png".into()];
    if let Some(output_name) = output_name {
        args.extend(["-o".into(), output_name.into()]);
    }
    // A final `-` writes the frame to stdout rather than creating a screenshot
    // file or touching the clipboard.
    args.push("-".into());
    args
}

/// Acquire the frozen selector background without producing a completed
/// screenshot notification. Portals remain the fallback for compositors that
/// do not expose `wlr-screencopy`.
pub async fn capture_for_selector(
    output_name: Option<&str>,
) -> Result<CapturedFrame, CaptureError> {
    match capture_via_grim(output_name) {
        Ok(frame) => Ok(frame),
        Err(screencopy_error) => capture_via_portal().await.map_err(|portal_error| {
            CaptureError::Unavailable(format!(
                "silent screencopy failed ({screencopy_error}); portal fallback failed: {portal_error}"
            ))
        }),
    }
}

/// Obtain a compositor frame through Niri's native screenshot IPC when the
/// generic Screenshot portal is unavailable. The selection remains Shilpo's
/// GPUI surface; Niri is used only as the compositor frame transport.
pub fn capture_via_niri() -> Result<CapturedFrame, CaptureError> {
    let directory = shilpo_config::CaptureConfig::default().resolved_screenshot_dir();
    std::fs::create_dir_all(&directory).map_err(|error| {
        CaptureError::Buffer(format!("could not create screenshot directory: {error}"))
    })?;
    let before = newest_png(&directory);
    let status = Command::new("niri")
        .args(["msg", "action", "screenshot-screen"])
        .status()
        .map_err(|error| {
            CaptureError::Unavailable(format!("could not invoke Niri screenshot action: {error}"))
        })?;
    if !status.success() {
        return Err(CaptureError::Rejected(format!(
            "Niri screenshot action exited with {status}"
        )));
    }
    let deadline = std::time::Instant::now() + Duration::from_secs(2);
    loop {
        if let Some(path) = newest_png(&directory)
            && (before.as_ref() != Some(&path) || before.is_none())
        {
            let image = image::open(&path)
                .map_err(|error| {
                    CaptureError::Buffer(format!("could not decode Niri screenshot: {error}"))
                })?
                .to_rgba8();
            let _ = std::fs::remove_file(path);
            return Ok(CapturedFrame {
                image,
                transform: super::OutputTransform::Normal,
                protocol: "niri-screenshot-ipc",
            });
        }
        if std::time::Instant::now() >= deadline {
            break;
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    Err(CaptureError::Protocol(
        "Niri screenshot action produced no new PNG".into(),
    ))
}

fn newest_png(directory: &std::path::Path) -> Option<std::path::PathBuf> {
    std::fs::read_dir(directory)
        .ok()?
        .filter_map(Result::ok)
        .filter(|entry| entry.path().extension().is_some_and(|ext| ext == "png"))
        .filter_map(|entry| {
            let modified = entry.metadata().ok()?.modified().ok()?;
            Some((modified, entry.path()))
        })
        .max_by_key(|(modified, _)| modified.duration_since(SystemTime::UNIX_EPOCH).ok())
        .map(|(_, path)| path)
}

impl CaptureBackend for WaylandCaptureBackend {
    fn name(&self) -> &'static str {
        "wayland"
    }

    fn protocol_name(&self) -> &'static str {
        "wlr-screencopy-v3"
    }

    fn outputs(&self) -> Vec<OutputInfo> {
        // Output geometry must come from the compositor's registry.  Until
        // the screencopy registry adapter has completed, returning no outputs
        // is intentional: callers must surface "capture unavailable" rather
        // than presenting a fabricated monitor and a fake frame.
        let _ = &self.conn;
        Vec::new()
    }

    fn capture_output(
        &self,
        output: &str,
        _with_cursor: bool,
    ) -> Result<CapturedFrame, CaptureError> {
        Err(CaptureError::Unavailable(format!(
            "Wayland screencopy capture is not available for output {output}; the protocol adapter is not initialized"
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::grim_capture_args;

    #[test]
    fn selector_screencopy_writes_only_to_stdout() {
        assert_eq!(
            grim_capture_args(Some("eDP-1")),
            ["-t", "png", "-o", "eDP-1", "-"]
        );
        assert_eq!(grim_capture_args(None), ["-t", "png", "-"]);
    }
}
