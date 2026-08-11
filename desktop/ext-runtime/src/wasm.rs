use crate::adapter::{ExtensionRuntime, RuntimeBudget, RuntimeError, RuntimeFailureKind};
use shilpo_ext_api::{ExtensionEvent, ExtensionId, HostEffect, ViewTree};
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::thread;
use std::time::Duration;
use wasmtime::component::{Component, Linker, ResourceTable, TypedFunc, types};
use wasmtime::{Config, Engine, Store, StoreLimits, StoreLimitsBuilder, Trap};
use wasmtime_wasi::{WasiCtx, WasiCtxBuilder, WasiCtxView, WasiView};

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

struct WasmState {
    limits: StoreLimits,
    table: ResourceTable,
    wasi: WasiCtx,
}

impl WasiView for WasmState {
    fn ctx(&mut self) -> WasiCtxView<'_> {
        WasiCtxView {
            ctx: &mut self.wasi,
            table: &mut self.table,
        }
    }
}

struct WasmInstance {
    store: Store<WasmState>,
    on_event: TypedFunc<(String,), (String,)>,
    view: TypedFunc<(String,), (String,)>,
}

pub struct WasmRuntime {
    engine: Engine,
    instances: HashMap<ExtensionId, WasmInstance>,
    ticker_stop: Arc<AtomicBool>,
    ticker: Option<thread::JoinHandle<()>>,
}

impl WasmRuntime {
    pub fn new() -> Result<Self, RuntimeError> {
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
        module: &WasmModule,
        budget: RuntimeBudget,
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
        let state = WasmState {
            limits: StoreLimitsBuilder::new()
                .memory_size(budget.max_memory_bytes)
                .instances(16)
                .memories(8)
                .tables(16)
                .build(),
            table: ResourceTable::new(),
            wasi: WasiCtxBuilder::new().build(),
        };
        let mut store = Store::new(&self.engine, state);
        store.limiter(|state| &mut state.limits);
        configure_store(&mut store, budget)?;
        let instance = linker
            .instantiate(&mut store, &component)
            .map_err(|error| classify_wasmtime_error("component instantiation failed", error))?;
        let on_event = instance
            .get_typed_func::<(String,), (String,)>(&mut store, "on-event")
            .map_err(|error| classify_wasmtime_error("missing on-event export", error))?;
        let view = instance
            .get_typed_func::<(String,), (String,)>(&mut store, "view")
            .map_err(|error| classify_wasmtime_error("missing view export", error))?;
        Ok(WasmInstance {
            store,
            on_event,
            view,
        })
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
    store.set_hostcall_fuel(budget.max_hostcall_bytes);
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

    fn compile_module(&self, bytes: &[u8]) -> Result<Self::Module, String> {
        Ok(WasmModule::from_bytes(bytes.to_vec()))
    }

    fn load(
        &mut self,
        extension_id: &ExtensionId,
        module: Self::Module,
        budget: RuntimeBudget,
    ) -> Result<(), RuntimeError> {
        if self.instances.contains_key(extension_id) {
            return Err(RuntimeError::with_kind(
                RuntimeFailureKind::Load,
                format!("extension '{extension_id}' is already loaded"),
            ));
        }

        let instance = self.instantiate_module(&module, budget)?;
        self.instances.insert(extension_id.clone(), instance);
        Ok(())
    }

    fn replace(
        &mut self,
        extension_id: &ExtensionId,
        module: Self::Module,
        budget: RuntimeBudget,
    ) -> Result<(), RuntimeError> {
        if !self.instances.contains_key(extension_id) {
            return Err(RuntimeError::with_kind(
                RuntimeFailureKind::Unavailable,
                format!("extension '{extension_id}' is not loaded"),
            ));
        }
        let replacement = self.instantiate_module(&module, budget)?;
        self.instances.insert(extension_id.clone(), replacement);
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
        event: &ExtensionEvent,
        budget: RuntimeBudget,
    ) -> Result<Vec<HostEffect>, RuntimeError> {
        let event_json = serde_json::to_string(event).map_err(|error| {
            RuntimeError::with_kind(
                RuntimeFailureKind::InvalidOutput,
                format!("failed to encode extension event: {error}"),
            )
        })?;
        let instance = self.instance_mut(extension_id)?;
        Self::prepare_call(instance, budget)?;
        let output = instance
            .on_event
            .call(&mut instance.store, (event_json,))
            .map_err(|error| classify_wasmtime_error("on-event call failed", error))?
            .0;
        if output.len() > budget.max_output_bytes {
            return Err(RuntimeError::with_kind(
                RuntimeFailureKind::InvalidOutput,
                "on-event output exceeds the configured byte limit",
            ));
        }
        serde_json::from_str(&output).map_err(|error| {
            RuntimeError::with_kind(
                RuntimeFailureKind::InvalidOutput,
                format!("on-event returned invalid effect JSON: {error}"),
            )
        })
    }

    fn view(
        &mut self,
        extension_id: &ExtensionId,
        contribution_id: &str,
        budget: RuntimeBudget,
    ) -> Result<Option<ViewTree>, RuntimeError> {
        let instance = self.instance_mut(extension_id)?;
        Self::prepare_call(instance, budget)?;
        let output = instance
            .view
            .call(&mut instance.store, (contribution_id.to_owned(),))
            .map_err(|error| classify_wasmtime_error("view call failed", error))?
            .0;
        if output.len() > budget.max_output_bytes {
            return Err(RuntimeError::with_kind(
                RuntimeFailureKind::InvalidOutput,
                "view output exceeds the configured byte limit",
            ));
        }
        serde_json::from_str(&output).map_err(|error| {
            RuntimeError::with_kind(
                RuntimeFailureKind::InvalidOutput,
                format!("view returned invalid JSON: {error}"),
            )
        })
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
        if !name.starts_with("wasi:") {
            return Err(RuntimeError::with_kind(
                RuntimeFailureKind::Load,
                format!("unsupported component import '{name}'"),
            ));
        }
    }
    validate_string_export(engine, &component_type, "on-event")?;
    validate_string_export(engine, &component_type, "view")
}

fn validate_string_export(
    engine: &Engine,
    component: &types::Component,
    name: &str,
) -> Result<(), RuntimeError> {
    let Some(export) = component.get_export(engine, name) else {
        return Err(RuntimeError::with_kind(
            RuntimeFailureKind::Load,
            format!("missing required component export '{name}'"),
        ));
    };
    let types::ComponentItem::ComponentFunc(function) = export.ty else {
        return Err(RuntimeError::with_kind(
            RuntimeFailureKind::Load,
            format!("component export '{name}' is not a function"),
        ));
    };
    let parameters = function.params().collect::<Vec<_>>();
    let results = function.results().collect::<Vec<_>>();
    if parameters.len() != 1
        || parameters[0].1 != types::Type::String
        || results != [types::Type::String]
    {
        return Err(RuntimeError::with_kind(
            RuntimeFailureKind::Load,
            format!("component export '{name}' must have type func(string) -> string"),
        ));
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
