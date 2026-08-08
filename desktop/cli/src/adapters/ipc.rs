use shilpo_services::{CompositorCommand, IpcRequest, IpcStatus, ServiceHealth, ShellIpcClient};

pub struct IpcAdapter {
    client: ShellIpcClient,
}

impl IpcAdapter {
    pub fn new() -> Self {
        Self {
            client: ShellIpcClient::new(),
        }
    }

    pub fn status(&self) -> Result<IpcStatus, (i32, String)> {
        self.client.status().map_err(map_ipc_error)
    }

    pub fn telemetry(&self) -> Result<ServiceHealth, (i32, String)> {
        self.client.telemetry().map_err(map_ipc_error)
    }

    pub fn request(&self, req: IpcRequest) -> Result<(), (i32, String)> {
        let resp = self.client.send(req).map_err(map_ipc_error)?;

        if !resp.ok {
            let err = resp.error.unwrap_or(shilpo_services::ipc::IpcErrorBody {
                code: "operation_failed".into(),
                message: "IPC operation failed".into(),
            });
            let code = match err.code.as_str() {
                "compositor_timeout" | "apply_timeout" | "timeout" => 4,
                _ => 1,
            };
            return Err((code, format!("{}: {}", err.code, err.message)));
        }

        Ok(())
    }

    pub fn overview_show(&self) -> Result<(), (i32, String)> {
        self.request(IpcRequest::ShowOverview)
    }

    pub fn overview_hide(&self) -> Result<(), (i32, String)> {
        self.request(IpcRequest::HideOverview)
    }

    pub fn overview_toggle(&self) -> Result<(), (i32, String)> {
        self.request(IpcRequest::ToggleOverview)
    }

    pub fn control_center_show(&self) -> Result<(), (i32, String)> {
        self.request(IpcRequest::ShowControlCenter)
    }

    pub fn control_center_hide(&self) -> Result<(), (i32, String)> {
        self.request(IpcRequest::HideControlCenter)
    }

    pub fn control_center_toggle(&self) -> Result<(), (i32, String)> {
        self.request(IpcRequest::ToggleControlCenter)
    }

    pub fn bar_show(&self) -> Result<(), (i32, String)> {
        self.request(IpcRequest::ShowBar)
    }

    pub fn bar_hide(&self) -> Result<(), (i32, String)> {
        self.request(IpcRequest::HideBar)
    }

    pub fn bar_toggle(&self) -> Result<(), (i32, String)> {
        self.request(IpcRequest::ToggleBar)
    }

    pub fn workspace_focus(&self, id: u64) -> Result<(), (i32, String)> {
        self.request(IpcRequest::Compositor(CompositorCommand::FocusWorkspace(
            id,
        )))
    }

    pub fn workspace_create(&self) -> Result<(), (i32, String)> {
        self.request(IpcRequest::Compositor(CompositorCommand::CreateWorkspace))
    }

    pub fn window_focus(&self, id: u64) -> Result<(), (i32, String)> {
        self.request(IpcRequest::Compositor(CompositorCommand::FocusWindow(id)))
    }

    pub fn window_focus_previous(&self) -> Result<(), (i32, String)> {
        self.request(IpcRequest::Compositor(
            CompositorCommand::FocusPreviousWindow,
        ))
    }

    pub fn window_move(&self, window_id: u64, workspace_id: u64) -> Result<(), (i32, String)> {
        self.request(IpcRequest::Compositor(
            CompositorCommand::MoveWindowToWorkspace {
                window_id,
                workspace_id,
            },
        ))
    }

    pub fn config_reload(&self) -> Result<(), (i32, String)> {
        self.request(IpcRequest::ReloadConfig)
    }

    pub fn capture(
        &self,
        intent: shilpo_capture::CaptureIntent,
    ) -> Result<shilpo_services::IpcResponse, (i32, String)> {
        self.checked_request(IpcRequest::Capture(intent))
    }

    pub fn record(
        &self,
        cmd: shilpo_capture::RecordingCommand,
    ) -> Result<shilpo_services::IpcResponse, (i32, String)> {
        self.checked_request(IpcRequest::Record(cmd))
    }

    fn checked_request(
        &self,
        request: IpcRequest,
    ) -> Result<shilpo_services::IpcResponse, (i32, String)> {
        let response = self.client.send(request).map_err(map_ipc_error)?;
        if response.ok {
            Ok(response)
        } else {
            let error = response
                .error
                .unwrap_or(shilpo_services::ipc::IpcErrorBody {
                    code: "operation_failed".into(),
                    message: "shell rejected the request".into(),
                });
            Err((1, format!("{}: {}", error.code, error.message)))
        }
    }
}

fn map_ipc_error(error: shilpo_services::IpcError) -> (i32, String) {
    let code = match &error {
        shilpo_services::IpcError::Code { code, .. }
            if code.contains("protocol") || code.contains("version") =>
        {
            5
        }
        shilpo_services::IpcError::Code { code, .. }
            if code.contains("auth") || code.contains("permission") =>
        {
            6
        }
        shilpo_services::IpcError::Code { code, .. } if code.contains("timeout") => 4,
        shilpo_services::IpcError::Io(_) | shilpo_services::IpcError::InvalidPath(_) => 3,
        _ => 1,
    };
    (code, format!("shell IPC error: {error}"))
}
