use crate::adapter::{ExtensionRuntime, RuntimeBudget, RuntimeError, RuntimeFailureKind};
use shilpo_ext_api::{
    Capability, ExtensionEvent as ApiEvent, ExtensionId, HostOperation, ViewTree as ApiViewTree,
    wildcard_matches,
};
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::thread;
use std::time::Duration;
use wasmtime::component::{Component, Linker, ResourceTable};
use wasmtime::{Config, Engine, Store, StoreLimits, StoreLimitsBuilder, Trap};
use wasmtime_wasi::{WasiCtx, WasiCtxBuilder, WasiCtxView, WasiView};

wasmtime::component::bindgen!({
    path: "../../core/ext-api/wit",
    world: "extension",
});

const EPOCH_TICK: Duration = Duration::from_millis(5);

#[derive(Clone, Debug)]
pub struct WasmModule {
    bytes: Arc<[u8]>,
}

impl WasmModule {
    pub fn from_bytes(bytes: impl Into<Vec<u8>>) -> Self {
        Self {
            bytes: bytes.into().into(),
        }
    }

    pub fn from_file(path: &Path) -> Result<Self, RuntimeError> {
        fs::read(path).map(Self::from_bytes).map_err(|error| {
            RuntimeError::with_kind(
                RuntimeFailureKind::Load,
                format!("failed to read WASM component {}: {error}", path.display()),
            )
        })
    }
}

pub type GrantChecker = Arc<dyn Fn(&ExtensionId, &str) -> bool + Send + Sync>;

pub struct WasmState {
    pub extension_id: ExtensionId,
    pub declared_capabilities: Vec<Capability>,
    pub granted_capabilities: Vec<Capability>,
    pub secret_broker: Arc<dyn crate::secrets::SecretBroker>,
    pub grant_checker: Option<GrantChecker>,
    pub limits: StoreLimits,
    pub table: ResourceTable,
    pub wasi: WasiCtx,
    pub operations: Vec<HostOperation>,
    pub state_store: HashMap<String, String>,
    pub hostcall_bytes: usize,
    pub max_hostcall_bytes: usize,
}

impl WasiView for WasmState {
    fn ctx(&mut self) -> WasiCtxView<'_> {
        WasiCtxView {
            ctx: &mut self.wasi,
            table: &mut self.table,
        }
    }
}
impl wasmtime::component::HasData for WasmState {
    type Data<'a> = &'a mut WasmState;
}

fn get_wasm_state(state: &mut WasmState) -> &mut WasmState {
    state
}

impl WasmState {
    fn check_capability(&self, predicate: impl Fn(&Capability) -> bool) -> bool {
        self.declared_capabilities.iter().any(&predicate)
            && self.granted_capabilities.iter().any(&predicate)
    }

    fn charge_hostcall_bytes(
        &mut self,
        bytes: usize,
    ) -> Result<(), shilpo::extension::types::Error> {
        self.hostcall_bytes += bytes;
        if self.hostcall_bytes > self.max_hostcall_bytes {
            return Err(shilpo::extension::types::Error {
                kind: shilpo::extension::types::ErrorKind::RateLimited,
                message: "host call byte limit exceeded".into(),
            });
        }
        Ok(())
    }
}

fn unauthorized_error(msg: impl Into<String>) -> shilpo::extension::types::Error {
    shilpo::extension::types::Error {
        kind: shilpo::extension::types::ErrorKind::Unauthorized,
        message: msg.into(),
    }
}

impl shilpo::extension::actions::Host for WasmState {
    fn invoke(
        &mut self,
        action_id: String,
        payload: Option<shilpo::extension::types::DataValue>,
    ) -> Result<(), shilpo::extension::types::Error> {
        let payload_json = payload
            .as_ref()
            .map(data_value_to_json)
            .map(|value| value.to_string());
        let call_bytes = action_id.len() + payload_json.as_ref().map_or(0, String::len);
        self.charge_hostcall_bytes(call_bytes)?;
        let allowed = self.check_capability(|cap| match cap {
            Capability::ActionsInvoke { actions } => actions
                .iter()
                .any(|pattern| wildcard_matches(pattern, &action_id)),
            _ => false,
        });
        if !allowed {
            return Err(unauthorized_error("actions:invoke capability denied"));
        }
        self.operations.push(HostOperation::InvokeAction {
            action_id,
            payload_json,
        });
        Ok(())
    }
}

impl shilpo::extension::clipboard::Host for WasmState {
    fn read(&mut self) -> Result<String, shilpo::extension::types::Error> {
        self.charge_hostcall_bytes(0)?;
        let allowed = self.check_capability(|cap| matches!(cap, Capability::ClipboardRead));
        if !allowed {
            return Err(unauthorized_error("clipboard:read capability denied"));
        }
        Ok(String::new())
    }

    fn write(&mut self, text: String) -> Result<(), shilpo::extension::types::Error> {
        self.charge_hostcall_bytes(text.len())?;
        let allowed = self.check_capability(|cap| matches!(cap, Capability::ClipboardWrite));
        if !allowed {
            return Err(unauthorized_error("clipboard:write capability denied"));
        }
        self.operations.push(HostOperation::ClipboardWrite { text });
        Ok(())
    }
}

impl shilpo::extension::filesystem::Host for WasmState {
    fn read_file(&mut self, path: String) -> Result<Vec<u8>, shilpo::extension::types::Error> {
        self.charge_hostcall_bytes(path.len())?;
        let allowed = self.check_capability(|cap| match cap {
            Capability::FilesystemRead { paths } => {
                paths.iter().any(|pattern| wildcard_matches(pattern, &path))
            }
            _ => false,
        });
        if !allowed {
            return Err(unauthorized_error("filesystem:read capability denied"));
        }
        Err(shilpo::extension::types::Error {
            kind: shilpo::extension::types::ErrorKind::NotFound,
            message: format!("virtual file '{path}' not found"),
        })
    }

    fn write_file(
        &mut self,
        path: String,
        contents: Vec<u8>,
    ) -> Result<(), shilpo::extension::types::Error> {
        self.charge_hostcall_bytes(path.len() + contents.len())?;
        let allowed = self.check_capability(|cap| match cap {
            Capability::FilesystemWrite { paths } => {
                paths.iter().any(|pattern| wildcard_matches(pattern, &path))
            }
            _ => false,
        });
        if !allowed {
            return Err(unauthorized_error("filesystem:write capability denied"));
        }
        Ok(())
    }
}

impl shilpo::extension::http::Host for WasmState {
    fn request(
        &mut self,
        req: shilpo::extension::http::HttpRequest,
    ) -> Result<String, shilpo::extension::types::Error> {
        let call_bytes =
            req.url.len() + req.method.len() + req.body.as_ref().map_or(0, |b| b.len());
        self.charge_hostcall_bytes(call_bytes)?;
        let target =
            crate::effects::CanonicalHttpTarget::parse(&req.url, &req.method).ok_or_else(|| {
                shilpo::extension::types::Error {
                    kind: shilpo::extension::types::ErrorKind::InvalidArgument,
                    message: "invalid HTTP request URL or method".into(),
                }
            })?;
        let allowed = self
            .check_capability(|cap| crate::effects::capability_allows_http_target(cap, &target));
        if !allowed {
            return Err(unauthorized_error("network:http capability denied"));
        }
        let req_id = req.request_id.clone();
        self.operations.push(HostOperation::HttpRequest {
            request_id: req.request_id,
            url: req.url,
            method: req.method,
        });
        Ok(req_id)
    }

    fn cancel(&mut self, _req_id: String) -> Result<(), shilpo::extension::types::Error> {
        self.charge_hostcall_bytes(0)?;
        Ok(())
    }
}

impl shilpo::extension::types::Host for WasmState {}
impl shilpo::extension::events::Host for WasmState {}
impl shilpo::extension::view::Host for WasmState {}

impl shilpo::extension::location::Host for WasmState {
    fn read(&mut self) -> Result<String, shilpo::extension::types::Error> {
        self.charge_hostcall_bytes(0)?;
        let allowed = self.check_capability(|cap| matches!(cap, Capability::LocationRead));
        if !allowed {
            return Err(unauthorized_error("location:read capability denied"));
        }
        let req_id = format!("loc-{}", self.operations.len() + 1);
        self.operations.push(HostOperation::LocationRead);
        Ok(req_id)
    }
}

impl shilpo::extension::notifications::Host for WasmState {
    fn show(
        &mut self,
        req: shilpo::extension::notifications::NotificationRequest,
    ) -> Result<(), shilpo::extension::types::Error> {
        let call_bytes =
            req.title.len() + req.body.len() + req.icon.as_ref().map_or(0, |i| i.len());
        self.charge_hostcall_bytes(call_bytes)?;
        let allowed = self.check_capability(|cap| matches!(cap, Capability::NotificationsShow));
        if !allowed {
            return Err(unauthorized_error("notifications:show capability denied"));
        }
        self.operations.push(HostOperation::ShowNotification {
            title: req.title,
            body: req.body,
            icon: req.icon,
        });
        Ok(())
    }
}

impl shilpo::extension::state::Host for WasmState {
    fn read(
        &mut self,
        key: String,
    ) -> Result<Option<shilpo::extension::types::DataValue>, shilpo::extension::types::Error> {
        self.charge_hostcall_bytes(key.len())?;
        Ok(self.state_store.get(&key).map(|value| {
            serde_json::from_str(value)
                .map(|value| data_value_from_json(&value))
                .unwrap_or_else(|_| shilpo::extension::types::DataValue::TextValue(value.clone()))
        }))
    }

    fn write(
        &mut self,
        key: String,
        value: shilpo::extension::types::DataValue,
    ) -> Result<(), shilpo::extension::types::Error> {
        let value_json = data_value_to_json(&value).to_string();
        self.charge_hostcall_bytes(key.len() + value_json.len())?;
        self.state_store.insert(key, value_json);
        Ok(())
    }
}

impl WasmState {
    fn check_secret_permission(&self, purpose: &shilpo_ext_api::SecretPurpose) -> bool {
        let declared = self.declared_capabilities.iter().any(
            |cap| matches!(cap, Capability::Secrets { purposes } if purposes.contains(purpose)),
        );
        if !declared {
            return false;
        }
        let scope = format!("secrets:{purpose}");
        if let Some(checker) = &self.grant_checker {
            checker(&self.extension_id, &scope)
        } else {
            self.granted_capabilities.iter().any(
                |cap| matches!(cap, Capability::Secrets { purposes } if purposes.contains(purpose)),
            )
        }
    }
}

fn map_secret_broker_error(
    err: crate::secrets::SecretBrokerError,
) -> shilpo::extension::types::Error {
    use crate::secrets::SecretBrokerError;
    use shilpo::extension::types as wit_types;

    match err {
        SecretBrokerError::BackendUnavailable(msg) => wit_types::Error {
            kind: wit_types::ErrorKind::BackendUnavailable,
            message: msg,
        },
        SecretBrokerError::Locked(msg) => wit_types::Error {
            kind: wit_types::ErrorKind::Locked,
            message: msg,
        },
        SecretBrokerError::Denied(msg) => wit_types::Error {
            kind: wit_types::ErrorKind::Denied,
            message: msg,
        },
        SecretBrokerError::Cancelled(msg) => wit_types::Error {
            kind: wit_types::ErrorKind::Cancelled,
            message: msg,
        },
        SecretBrokerError::NotFound(msg) | SecretBrokerError::InvalidReference(msg) => {
            wit_types::Error {
                kind: wit_types::ErrorKind::NotFound,
                message: msg,
            }
        }
        SecretBrokerError::Internal(msg) => wit_types::Error {
            kind: wit_types::ErrorKind::Internal,
            message: msg,
        },
    }
}

impl shilpo::extension::secrets::Host for WasmState {
    fn set(
        &mut self,
        purpose: String,
        value: Vec<u8>,
    ) -> Result<shilpo::extension::secrets::SecretRef, shilpo::extension::types::Error> {
        self.charge_hostcall_bytes(purpose.len() + value.len())?;
        let parsed_purpose = shilpo_ext_api::SecretPurpose::parse(&purpose).map_err(|err| {
            shilpo::extension::types::Error {
                kind: shilpo::extension::types::ErrorKind::InvalidArgument,
                message: err.to_string(),
            }
        })?;

        if !self.check_secret_permission(&parsed_purpose) {
            return Err(shilpo::extension::types::Error {
                kind: shilpo::extension::types::ErrorKind::Unauthorized,
                message: format!(
                    "extension '{}' missing declaration or grant for secret purpose '{}'",
                    self.extension_id, parsed_purpose
                ),
            });
        }

        let reference = self
            .secret_broker
            .set(&self.extension_id, &parsed_purpose, &value)
            .map_err(map_secret_broker_error)?;

        Ok(shilpo::extension::secrets::SecretRef {
            handle: reference.handle,
        })
    }

    fn read(
        &mut self,
        purpose: String,
        reference: shilpo::extension::secrets::SecretRef,
    ) -> Result<Option<Vec<u8>>, shilpo::extension::types::Error> {
        self.charge_hostcall_bytes(purpose.len() + reference.handle.len())?;
        let parsed_purpose = shilpo_ext_api::SecretPurpose::parse(&purpose).map_err(|err| {
            shilpo::extension::types::Error {
                kind: shilpo::extension::types::ErrorKind::InvalidArgument,
                message: err.to_string(),
            }
        })?;

        if !self.check_secret_permission(&parsed_purpose) {
            return Err(shilpo::extension::types::Error {
                kind: shilpo::extension::types::ErrorKind::Unauthorized,
                message: format!(
                    "extension '{}' missing declaration or grant for secret purpose '{}'",
                    self.extension_id, parsed_purpose
                ),
            });
        }

        let api_ref = shilpo_ext_api::SecretRef::new(reference.handle);
        let bytes = self
            .secret_broker
            .read(&self.extension_id, &parsed_purpose, &api_ref)
            .map_err(map_secret_broker_error)?;

        Ok(bytes)
    }

    fn delete(
        &mut self,
        purpose: String,
        reference: shilpo::extension::secrets::SecretRef,
    ) -> Result<(), shilpo::extension::types::Error> {
        self.charge_hostcall_bytes(purpose.len() + reference.handle.len())?;
        let parsed_purpose = shilpo_ext_api::SecretPurpose::parse(&purpose).map_err(|err| {
            shilpo::extension::types::Error {
                kind: shilpo::extension::types::ErrorKind::InvalidArgument,
                message: err.to_string(),
            }
        })?;

        if !self.check_secret_permission(&parsed_purpose) {
            return Err(shilpo::extension::types::Error {
                kind: shilpo::extension::types::ErrorKind::Unauthorized,
                message: format!(
                    "extension '{}' missing declaration or grant for secret purpose '{}'",
                    self.extension_id, parsed_purpose
                ),
            });
        }

        let api_ref = shilpo_ext_api::SecretRef::new(reference.handle);
        self.secret_broker
            .delete(&self.extension_id, &parsed_purpose, &api_ref)
            .map_err(map_secret_broker_error)?;

        Ok(())
    }
}

impl shilpo::extension::theme::Host for WasmState {
    fn read(
        &mut self,
    ) -> Result<shilpo::extension::theme::ThemeInfo, shilpo::extension::types::Error> {
        self.charge_hostcall_bytes(0)?;
        let allowed = self.check_capability(|cap| matches!(cap, Capability::ThemeRead));
        if !allowed {
            return Err(unauthorized_error("theme:read capability denied"));
        }
        Ok(shilpo::extension::theme::ThemeInfo {
            mode: "dark".into(),
            accent: "#006c4c".into(),
        })
    }

    fn set_source_color(&mut self, color: String) -> Result<(), shilpo::extension::types::Error> {
        self.charge_hostcall_bytes(color.len())?;
        let allowed = self.check_capability(|cap| matches!(cap, Capability::ThemeSetSource));
        if !allowed {
            return Err(unauthorized_error("theme:set_source capability denied"));
        }
        self.operations
            .push(HostOperation::SetThemeSource { color });
        Ok(())
    }
}

impl shilpo::extension::wallpaper::Host for WasmState {
    fn read(
        &mut self,
    ) -> Result<shilpo::extension::wallpaper::WallpaperInfo, shilpo::extension::types::Error> {
        self.charge_hostcall_bytes(0)?;
        let allowed = self.check_capability(|cap| matches!(cap, Capability::WallpaperRead));
        if !allowed {
            return Err(unauthorized_error("wallpaper:read capability denied"));
        }
        Ok(shilpo::extension::wallpaper::WallpaperInfo {
            path: String::new(),
        })
    }

    fn set(
        &mut self,
        path: String,
        source: shilpo::extension::wallpaper::WallpaperSource,
    ) -> Result<(), shilpo::extension::types::Error> {
        self.charge_hostcall_bytes(path.len())?;
        let api_source = match source {
            shilpo::extension::wallpaper::WallpaperSource::ExtensionAsset => {
                shilpo_ext_api::WallpaperSource::ExtensionAsset
            }
            shilpo::extension::wallpaper::WallpaperSource::LocalFile => {
                shilpo_ext_api::WallpaperSource::LocalFile
            }
            shilpo::extension::wallpaper::WallpaperSource::Remote => {
                shilpo_ext_api::WallpaperSource::Remote
            }
        };
        let allowed = self.check_capability(|cap| match cap {
            Capability::WallpaperSet { sources } => sources.contains(&api_source),
            _ => false,
        });
        if !allowed {
            return Err(unauthorized_error("wallpaper:set capability denied"));
        }
        self.operations.push(HostOperation::SetWallpaper {
            path,
            source: api_source,
        });
        Ok(())
    }
}

struct WasmInstance {
    store: Store<WasmState>,
    extension: Extension,
}

pub struct WasmRuntime {
    engine: Engine,
    instances: HashMap<ExtensionId, WasmInstance>,
    pub secret_broker: Arc<dyn crate::secrets::SecretBroker>,
    pub grant_checker: Option<GrantChecker>,
    ticker_stop: Arc<AtomicBool>,
    ticker: Option<thread::JoinHandle<()>>,
}

impl WasmRuntime {
    pub fn new() -> Result<Self, RuntimeError> {
        let broker: Arc<dyn crate::secrets::SecretBroker> =
            match crate::secrets::Oo7SecretBroker::new() {
                Ok(broker) => Arc::new(broker),
                Err(_) => Arc::new(crate::secrets::FakeSecretBroker::new()),
            };
        Self::with_broker(broker)
    }

    pub fn with_broker(
        secret_broker: Arc<dyn crate::secrets::SecretBroker>,
    ) -> Result<Self, RuntimeError> {
        Self::with_broker_and_grant_checker(secret_broker, None)
    }

    pub fn with_broker_and_grant_checker(
        secret_broker: Arc<dyn crate::secrets::SecretBroker>,
        grant_checker: Option<GrantChecker>,
    ) -> Result<Self, RuntimeError> {
        let engine = configured_engine()?;
        let ticker_stop = Arc::new(AtomicBool::new(false));
        let ticker_engine = engine.clone();
        let ticker_flag = ticker_stop.clone();
        let ticker = thread::Builder::new()
            .name("shilpo-extension-epoch".into())
            .spawn(move || {
                while !ticker_flag.load(Ordering::Relaxed) {
                    thread::sleep(EPOCH_TICK);
                    ticker_engine.increment_epoch();
                }
            })
            .map_err(|error| {
                RuntimeError::with_kind(
                    RuntimeFailureKind::Load,
                    format!("failed to start WASM deadline ticker: {error}"),
                )
            })?;

        Ok(Self {
            engine,
            instances: HashMap::new(),
            secret_broker,
            grant_checker,
            ticker_stop,
            ticker: Some(ticker),
        })
    }

    pub fn validate_module(bytes: &[u8]) -> Result<(), RuntimeError> {
        let engine = configured_engine()?;
        let component = compile_component(&engine, bytes)?;
        validate_component_type(&engine, &component)
    }

    fn prepare_call(
        instance: &mut WasmInstance,
        budget: RuntimeBudget,
    ) -> Result<(), RuntimeError> {
        configure_store(&mut instance.store, budget)
    }

    fn instance_mut(
        &mut self,
        extension_id: &ExtensionId,
    ) -> Result<&mut WasmInstance, RuntimeError> {
        self.instances.get_mut(extension_id).ok_or_else(|| {
            RuntimeError::with_kind(
                RuntimeFailureKind::Unavailable,
                format!("extension '{extension_id}' is not loaded"),
            )
        })
    }

    fn instantiate_module(
        &self,
        extension_id: &ExtensionId,
        module: &WasmModule,
        budget: RuntimeBudget,
        declared_capabilities: Vec<Capability>,
        granted_capabilities: Vec<Capability>,
    ) -> Result<WasmInstance, RuntimeError> {
        let component = compile_component(&self.engine, &module.bytes)?;
        validate_component_type(&self.engine, &component)?;
        let mut linker = Linker::<WasmState>::new(&self.engine);
        wasmtime_wasi::p2::add_to_linker_sync(&mut linker).map_err(|error| {
            RuntimeError::with_kind(
                RuntimeFailureKind::Load,
                format!("failed to configure sandboxed WASI imports: {error:#}"),
            )
        })?;
        Extension::add_to_linker::<WasmState, WasmState>(
            &mut linker,
            get_wasm_state as fn(&mut WasmState) -> &mut WasmState,
        )
        .map_err(|error| {
            RuntimeError::with_kind(
                RuntimeFailureKind::Load,
                format!("failed to configure component imports: {error:#}"),
            )
        })?;
        let state = WasmState {
            extension_id: extension_id.clone(),
            declared_capabilities,
            granted_capabilities,
            secret_broker: self.secret_broker.clone(),
            grant_checker: self.grant_checker.clone(),
            limits: StoreLimitsBuilder::new()
                .memory_size(budget.max_memory_bytes)
                .instances(16)
                .memories(8)
                .tables(16)
                .build(),
            table: ResourceTable::new(),
            wasi: WasiCtxBuilder::new().build(),
            operations: Vec::new(),
            state_store: HashMap::new(),
            hostcall_bytes: 0,
            max_hostcall_bytes: budget.max_hostcall_bytes,
        };
        let mut store = Store::new(&self.engine, state);
        store.limiter(|state| &mut state.limits);
        configure_store(&mut store, budget)?;

        let extension = Extension::instantiate(&mut store, &component, &linker)
            .map_err(|error| classify_wasmtime_error("component instantiation failed", error))?;
        Ok(WasmInstance { store, extension })
    }

    pub fn load_with_capabilities(
        &mut self,
        extension_id: &ExtensionId,
        module: WasmModule,
        budget: RuntimeBudget,
        declared_capabilities: Vec<Capability>,
        granted_capabilities: Vec<Capability>,
    ) -> Result<(), RuntimeError> {
        if self.instances.contains_key(extension_id) {
            return Err(RuntimeError::with_kind(
                RuntimeFailureKind::Load,
                format!("extension '{extension_id}' is already loaded"),
            ));
        }
        let instance = self.instantiate_module(
            extension_id,
            &module,
            budget,
            declared_capabilities,
            granted_capabilities,
        )?;
        self.instances.insert(extension_id.clone(), instance);
        Ok(())
    }
}

fn configure_store(
    store: &mut Store<WasmState>,
    budget: RuntimeBudget,
) -> Result<(), RuntimeError> {
    store.set_fuel(budget.fuel).map_err(|error| {
        RuntimeError::with_kind(
            RuntimeFailureKind::Load,
            format!("failed to configure WASM fuel: {error}"),
        )
    })?;
    store.data_mut().hostcall_bytes = 0;
    store.data_mut().max_hostcall_bytes = budget.max_hostcall_bytes;
    let ticks = budget
        .deadline
        .as_nanos()
        .div_ceil(EPOCH_TICK.as_nanos())
        .max(1) as u64;
    store.set_epoch_deadline(ticks);
    Ok(())
}

impl ExtensionRuntime for WasmRuntime {
    type Module = WasmModule;

    fn load_with_capabilities(
        &mut self,
        extension_id: &ExtensionId,
        module: Self::Module,
        budget: RuntimeBudget,
        declared_capabilities: Vec<Capability>,
        granted_capabilities: Vec<Capability>,
    ) -> Result<(), RuntimeError> {
        self.load_with_capabilities(
            extension_id,
            module,
            budget,
            declared_capabilities,
            granted_capabilities,
        )
    }

    fn replace_with_capabilities(
        &mut self,
        extension_id: &ExtensionId,
        module: Self::Module,
        budget: RuntimeBudget,
        declared_capabilities: Vec<Capability>,
        granted_capabilities: Vec<Capability>,
    ) -> Result<(), RuntimeError> {
        let replacement = self.instantiate_module(
            extension_id,
            &module,
            budget,
            declared_capabilities,
            granted_capabilities,
        )?;
        self.instances.insert(extension_id.clone(), replacement);
        Ok(())
    }

    fn compile_module(&self, bytes: &[u8]) -> Result<Self::Module, String> {
        Ok(WasmModule::from_bytes(bytes.to_vec()))
    }

    fn load(
        &mut self,
        extension_id: &ExtensionId,
        module: Self::Module,
        budget: RuntimeBudget,
    ) -> Result<(), RuntimeError> {
        self.load_with_capabilities(extension_id, module, budget, Vec::new(), Vec::new())
    }

    fn replace(
        &mut self,
        extension_id: &ExtensionId,
        module: Self::Module,
        budget: RuntimeBudget,
    ) -> Result<(), RuntimeError> {
        let span = tracing::info_span!(
            target: "shilpo_profile",
            "extension_wasm_call",
            extension_id = %extension_id,
            operation = "replace",
            fuel = budget.fuel,
            memory_limit = budget.max_memory_bytes,
            outcome = "failure",
        );
        let _enter = span.enter();
        let (declared, granted) = self
            .instances
            .get(extension_id)
            .map(|i| {
                (
                    i.store.data().declared_capabilities.clone(),
                    i.store.data().granted_capabilities.clone(),
                )
            })
            .ok_or_else(|| {
                RuntimeError::with_kind(
                    RuntimeFailureKind::Unavailable,
                    format!("extension '{extension_id}' is not loaded"),
                )
            })?;
        let replacement =
            self.instantiate_module(extension_id, &module, budget, declared, granted)?;
        self.instances.insert(extension_id.clone(), replacement);
        span.record("outcome", "success");
        Ok(())
    }

    fn unload(&mut self, extension_id: &ExtensionId) -> Result<(), RuntimeError> {
        self.instances
            .remove(extension_id)
            .map(|_| ())
            .ok_or_else(|| {
                RuntimeError::with_kind(
                    RuntimeFailureKind::Unavailable,
                    format!("extension '{extension_id}' is not loaded"),
                )
            })
    }

    fn dispatch(
        &mut self,
        extension_id: &ExtensionId,
        event: &ApiEvent,
        budget: RuntimeBudget,
    ) -> Result<Vec<HostOperation>, RuntimeError> {
        let span = tracing::info_span!(
            target: "shilpo_profile",
            "extension_wasm_call",
            extension_id = %extension_id,
            operation = "dispatch",
            fuel = budget.fuel,
            memory_limit = budget.max_memory_bytes,
            outcome = "failure",
        );
        let _enter = span.enter();
        let instance = self.instance_mut(extension_id)?;
        Self::prepare_call(instance, budget)?;
        instance.store.data_mut().operations.clear();
        let wit_event = convert_event_to_wit(event);
        let result = instance
            .extension
            .call_on_event(&mut instance.store, &wit_event)
            .map_err(|error| classify_wasmtime_error("on-event call failed", error))?;
        match result {
            Ok(()) => {
                let ops = instance.store.data_mut().operations.drain(..).collect();
                span.record("outcome", "success");
                Ok(ops)
            }
            Err(err) => Err(RuntimeError::with_kind(
                RuntimeFailureKind::InvalidOutput,
                format!("on-event failed: {}", err.message),
            )),
        }
    }

    fn view(
        &mut self,
        extension_id: &ExtensionId,
        contribution_id: &str,
        budget: RuntimeBudget,
    ) -> Result<Option<ApiViewTree>, RuntimeError> {
        let span = tracing::info_span!(
            target: "shilpo_profile",
            "extension_wasm_call",
            extension_id = %extension_id,
            operation = "view",
            fuel = budget.fuel,
            memory_limit = budget.max_memory_bytes,
            outcome = "failure",
        );
        let _enter = span.enter();
        let instance = self.instance_mut(extension_id)?;
        Self::prepare_call(instance, budget)?;
        let result = instance
            .extension
            .call_view(&mut instance.store, contribution_id)
            .map_err(|error| classify_wasmtime_error("view call failed", error))?;
        match result {
            Ok(Some(tree)) => {
                span.record("outcome", "success");
                Ok(Some(convert_view_tree_from_wit(tree)))
            }
            Ok(None) => {
                span.record("outcome", "success");
                Ok(None)
            }
            Err(err) => Err(RuntimeError::with_kind(
                RuntimeFailureKind::InvalidOutput,
                format!("view failed: {}", err.message),
            )),
        }
    }
}

impl Drop for WasmRuntime {
    fn drop(&mut self) {
        self.ticker_stop.store(true, Ordering::Relaxed);
        if let Some(ticker) = self.ticker.take() {
            let _ = ticker.join();
        }
    }
}

fn configured_engine() -> Result<Engine, RuntimeError> {
    let mut config = Config::new();
    config.wasm_component_model(true);
    config.consume_fuel(true);
    config.epoch_interruption(true);
    config.max_wasm_stack(512 * 1024);
    Engine::new(&config).map_err(|error| {
        RuntimeError::with_kind(
            RuntimeFailureKind::Load,
            format!("failed to configure Wasmtime: {error}"),
        )
    })
}

fn compile_component(engine: &Engine, bytes: &[u8]) -> Result<Component, RuntimeError> {
    Component::new(engine, bytes).map_err(|error| {
        RuntimeError::with_kind(
            RuntimeFailureKind::Load,
            format!("component compilation failed: {error:#}"),
        )
    })
}

fn validate_component_type(engine: &Engine, component: &Component) -> Result<(), RuntimeError> {
    let component_type = component.component_type();
    for (name, _) in component_type.imports(engine) {
        if !name.starts_with("wasi:") && !name.starts_with("shilpo:extension") {
            return Err(RuntimeError::with_kind(
                RuntimeFailureKind::Load,
                format!("unsupported component import '{name}'"),
            ));
        }
    }
    for export in ["activate", "deactivate", "on-event", "view"] {
        if component_type.get_export(engine, export).is_none() {
            return Err(RuntimeError::with_kind(
                RuntimeFailureKind::Load,
                format!("missing required component export '{export}'"),
            ));
        }
    }
    Ok(())
}

fn classify_wasmtime_error(context: &str, error: wasmtime::Error) -> RuntimeError {
    let message = format!("{error:#}");
    let lowercase = message.to_ascii_lowercase();
    let kind = match error.downcast_ref::<Trap>() {
        Some(Trap::OutOfFuel) => RuntimeFailureKind::FuelExhausted,
        Some(Trap::Interrupt) => RuntimeFailureKind::Timeout,
        Some(Trap::MemoryOutOfBounds | Trap::AllocationTooLarge) => RuntimeFailureKind::MemoryLimit,
        _ if lowercase.contains("fuel") => RuntimeFailureKind::FuelExhausted,
        _ if lowercase.contains("epoch") || lowercase.contains("interrupt") => {
            RuntimeFailureKind::Timeout
        }
        _ if lowercase.contains("memory") || lowercase.contains("resource limit") => {
            RuntimeFailureKind::MemoryLimit
        }
        _ => RuntimeFailureKind::Trap,
    };
    RuntimeError::with_kind(kind, format!("{context}: {message}"))
}

fn data_value_from_json(value: &serde_json::Value) -> self::shilpo::extension::types::DataValue {
    use self::shilpo::extension::types::DataValue;
    match value {
        serde_json::Value::Null => DataValue::None,
        serde_json::Value::Bool(value) => DataValue::BoolValue(*value),
        serde_json::Value::Number(value) => value
            .as_i64()
            .map(DataValue::IntValue)
            .or_else(|| value.as_f64().map(DataValue::FloatValue))
            .unwrap_or_else(|| DataValue::TextValue(value.to_string())),
        serde_json::Value::String(value) => DataValue::TextValue(value.clone()),
        value => DataValue::BytesValue(value.to_string().into_bytes()),
    }
}

fn data_value_to_json(value: &self::shilpo::extension::types::DataValue) -> serde_json::Value {
    use self::shilpo::extension::types::DataValue;
    match value {
        DataValue::None => serde_json::Value::Null,
        DataValue::BoolValue(value) => serde_json::Value::Bool(*value),
        DataValue::IntValue(value) => serde_json::json!(*value),
        DataValue::FloatValue(value) => serde_json::json!(*value),
        DataValue::TextValue(value) => serde_json::Value::String(value.clone()),
        DataValue::BytesValue(value) => serde_json::Value::Array(
            value
                .iter()
                .map(|value| serde_json::json!(*value))
                .collect(),
        ),
        DataValue::SecretRef(s) => serde_json::json!({ "secret_ref": s.handle }),
    }
}

fn convert_event_to_wit(event: &ApiEvent) -> self::shilpo::extension::events::ExtensionEvent {
    use self::shilpo::extension::events as wit_events;
    match event {
        ApiEvent::ShellStarted => wit_events::ExtensionEvent::ShellStarted,
        ApiEvent::ShellStopping => wit_events::ExtensionEvent::ShellStopping,
        ApiEvent::OutputsChanged => wit_events::ExtensionEvent::OutputsChanged,
        ApiEvent::ThemeChanged { mode } => {
            wit_events::ExtensionEvent::ThemeChanged(wit_events::ThemeEvent { mode: mode.clone() })
        }
        ApiEvent::PaletteGenerated { accent } => {
            wit_events::ExtensionEvent::PaletteGenerated(wit_events::PaletteEvent {
                accent: accent.clone(),
            })
        }
        ApiEvent::WallpaperChanged { path } => {
            wit_events::ExtensionEvent::WallpaperChanged(wit_events::WallpaperEvent {
                path: path.clone(),
            })
        }
        ApiEvent::NetworkChanged { connected } => {
            wit_events::ExtensionEvent::NetworkChanged(wit_events::NetworkEvent {
                connected: *connected,
            })
        }
        ApiEvent::MediaChanged {
            title,
            artist,
            playing,
        } => wit_events::ExtensionEvent::MediaChanged(wit_events::MediaEvent {
            title: title.clone(),
            artist: artist.clone(),
            playing: *playing,
        }),
        ApiEvent::PowerChanged {
            percentage,
            charging,
        } => wit_events::ExtensionEvent::PowerChanged(wit_events::PowerEvent {
            percentage: *percentage,
            charging: *charging,
        }),
        ApiEvent::TimerFired { name } => {
            wit_events::ExtensionEvent::TimerFired(wit_events::TimerEvent { name: name.clone() })
        }
        ApiEvent::ContributionMounted {
            contribution_id,
            instance_id,
            width,
            height,
        } => wit_events::ExtensionEvent::ContributionMounted(wit_events::MountedEvent {
            contribution_id: contribution_id.clone(),
            instance_id: instance_id.clone(),
            width: *width,
            height: *height,
        }),
        ApiEvent::ContributionUnmounted {
            contribution_id,
            instance_id,
        } => wit_events::ExtensionEvent::ContributionUnmounted(wit_events::UnmountedEvent {
            contribution_id: contribution_id.clone(),
            instance_id: instance_id.clone(),
        }),
        ApiEvent::ContributionResized {
            contribution_id,
            instance_id,
            width,
            height,
        } => wit_events::ExtensionEvent::ContributionResized(wit_events::ResizedEvent {
            contribution_id: contribution_id.clone(),
            instance_id: instance_id.clone(),
            width: *width,
            height: *height,
        }),
        ApiEvent::ContributionSettingsChanged {
            contribution_id,
            instance_id,
            settings,
        } => wit_events::ExtensionEvent::ContributionSettingsChanged(wit_events::SettingsEvent {
            contribution_id: contribution_id.clone(),
            instance_id: instance_id.clone(),
            settings: data_value_from_json(settings),
        }),
        ApiEvent::Input {
            contribution_id,
            instance_id,
            event_id,
            value,
        } => wit_events::ExtensionEvent::Input(wit_events::InputEvent {
            contribution_id: contribution_id.clone(),
            instance_id: instance_id.clone(),
            event_id: event_id.clone(),
            value: value.as_ref().map(data_value_from_json),
        }),
        ApiEvent::StateValue { key, value } => {
            wit_events::ExtensionEvent::StateValue(wit_events::StateEvent {
                key: key.clone(),
                value: value.as_ref().map(data_value_from_json),
            })
        }
        ApiEvent::HttpResponse {
            request_id,
            status,
            body,
            error,
        } => wit_events::ExtensionEvent::HttpResponse(wit_events::HttpResponseEvent {
            request_id: request_id.clone(),
            status: *status,
            body: body.clone(),
            error: error.clone(),
        }),
        ApiEvent::LocationResponse {
            latitude,
            longitude,
            accuracy_meters,
            error,
        } => wit_events::ExtensionEvent::LocationResponse(wit_events::LocationResponseEvent {
            latitude: *latitude,
            longitude: *longitude,
            accuracy_meters: *accuracy_meters,
            error: error.clone(),
        }),
    }
}

fn convert_view_tree_from_wit(tree: self::shilpo::extension::view::ViewTree) -> ApiViewTree {
    fn decode_node(
        idx: usize,
        nodes: &[self::shilpo::extension::view::ViewNode],
    ) -> Option<shilpo_ext_api::ViewNode> {
        use self::shilpo::extension::view as wit_view;
        use shilpo_ext_api as api;
        if idx >= nodes.len() {
            return None;
        }
        let node = match &nodes[idx] {
            wit_view::ViewNode::Container(c) => api::ViewNode::Container(api::ContainerNode {
                direction: match c.direction {
                    wit_view::ContainerDirection::Row => api::ContainerDirection::Row,
                    wit_view::ContainerDirection::Column => api::ContainerDirection::Column,
                    wit_view::ContainerDirection::Stack => api::ContainerDirection::Stack,
                },
                children: c
                    .children
                    .iter()
                    .filter_map(|&child_idx| decode_node(child_idx as usize, nodes))
                    .collect(),
                style: c.style.as_ref().map(decode_style),
                gap: c.gap,
            }),
            wit_view::ViewNode::Text(t) => api::ViewNode::Text(api::TextNode {
                content: t.content.clone(),
                font_size: t.font_size,
                bold: t.bold,
                style: t.style.as_ref().map(decode_style),
            }),
            wit_view::ViewNode::Icon(i) => api::ViewNode::Icon(api::IconNode {
                name: i.name.clone(),
                size: i.size,
                style: i.style.as_ref().map(decode_style),
            }),
            wit_view::ViewNode::Image(img) => api::ViewNode::Image(api::ImageNode {
                asset_path: img.asset_path.clone(),
                width: img.width,
                height: img.height,
                style: img.style.as_ref().map(decode_style),
            }),
            wit_view::ViewNode::Button(b) => api::ViewNode::Button(api::ButtonNode {
                label: b.label.clone(),
                event_id: b.event_id.clone(),
                style: b.style.as_ref().map(decode_style),
            }),
            wit_view::ViewNode::IconButton(ib) => api::ViewNode::IconButton(api::IconButtonNode {
                icon_name: ib.icon_name.clone(),
                event_id: ib.event_id.clone(),
                style: ib.style.as_ref().map(decode_style),
            }),
            wit_view::ViewNode::Toggle(t) => api::ViewNode::Toggle(api::ToggleNode {
                value: t.value,
                event_id: t.event_id.clone(),
                style: t.style.as_ref().map(decode_style),
            }),
            wit_view::ViewNode::Slider(s) => api::ViewNode::Slider(api::SliderNode {
                value: s.value,
                min: s.min,
                max: s.max,
                event_id: s.event_id.clone(),
                style: s.style.as_ref().map(decode_style),
            }),
            wit_view::ViewNode::TextInput(ti) => api::ViewNode::TextInput(api::TextInputNode {
                placeholder: ti.placeholder.clone(),
                value: ti.value.clone(),
                event_id: ti.event_id.clone(),
                style: ti.style.as_ref().map(decode_style),
            }),
            wit_view::ViewNode::List(l) => api::ViewNode::List(api::ListNode {
                items: l
                    .items
                    .iter()
                    .filter_map(|&item_idx| decode_node(item_idx as usize, nodes))
                    .collect(),
                style: l.style.as_ref().map(decode_style),
            }),
            wit_view::ViewNode::Spacer(sp) => {
                api::ViewNode::Spacer(api::SpacerNode { size: sp.size })
            }
            wit_view::ViewNode::Divider => api::ViewNode::Divider,
            wit_view::ViewNode::Badge(bd) => api::ViewNode::Badge(api::BadgeNode {
                label: bd.label.clone(),
                style: bd.style.as_ref().map(decode_style),
            }),
            wit_view::ViewNode::Progress(pr) => api::ViewNode::Progress(api::ProgressNode {
                value: pr.value,
                style: pr.style.as_ref().map(decode_style),
            }),
            wit_view::ViewNode::LoadingIndicator(li) => {
                api::ViewNode::LoadingIndicator(api::LoadingIndicatorNode {
                    size: li.size,
                    color: li.color.map(decode_color),
                    style: li.style.as_ref().map(decode_style),
                })
            }
        };
        Some(node)
    }

    fn decode_style(s: &self::shilpo::extension::view::ViewStyle) -> shilpo_ext_api::ViewStyle {
        shilpo_ext_api::ViewStyle {
            padding: s.padding,
            margin: s.margin,
            width: s.width,
            height: s.height,
            corner_radius: s.corner_radius,
            opacity: s.opacity,
            color: s.color.map(decode_color),
            background: s.background.map(decode_color),
            flex_grow: s.flex_grow,
        }
    }

    fn decode_color(
        token: self::shilpo::extension::view::SemanticColorToken,
    ) -> shilpo_ext_api::SemanticColorToken {
        use self::shilpo::extension::view as wit_view;
        use shilpo_ext_api as api;
        match token {
            wit_view::SemanticColorToken::Primary => api::SemanticColorToken::Primary,
            wit_view::SemanticColorToken::OnPrimary => api::SemanticColorToken::OnPrimary,
            wit_view::SemanticColorToken::Secondary => api::SemanticColorToken::Secondary,
            wit_view::SemanticColorToken::Surface => api::SemanticColorToken::Surface,
            wit_view::SemanticColorToken::SurfaceContainer => {
                api::SemanticColorToken::SurfaceContainer
            }
            wit_view::SemanticColorToken::OnSurface => api::SemanticColorToken::OnSurface,
            wit_view::SemanticColorToken::OnSurfaceVariant => {
                api::SemanticColorToken::OnSurfaceVariant
            }
            wit_view::SemanticColorToken::Outline => api::SemanticColorToken::Outline,
            wit_view::SemanticColorToken::Error => api::SemanticColorToken::Error,
        }
    }

    let root =
        decode_node(tree.root as usize, &tree.nodes).unwrap_or(shilpo_ext_api::ViewNode::Divider);
    ApiViewTree::new(root)
}
