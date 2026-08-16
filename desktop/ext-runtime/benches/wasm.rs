use criterion::{Criterion, black_box, criterion_group, criterion_main};
use shilpo_ext_api::ExtensionId;
use shilpo_ext_runtime::{
    ExtensionRuntime, FakeSecretBroker, RuntimeBudget, WasmModule, WasmRuntime,
};
use std::sync::Arc;
use std::time::{Duration, Instant};

const SDK_FIXTURE: &[u8] = include_bytes!(env!("SHILPO_SDK_FIXTURE_WASM"));

fn bench_wasm_cold_load(c: &mut Criterion) {
    let broker = Arc::new(FakeSecretBroker::new());
    let mut runtime =
        WasmRuntime::with_broker(broker).expect("WasmRuntime with fake broker must succeed");
    let module = WasmModule::from_bytes(SDK_FIXTURE);
    let budget = RuntimeBudget::default();
    let ext_id = ExtensionId::new("io.shilpo.bench-cold-load").unwrap();

    // Verify initial load and unload outside timing
    let test_id = ExtensionId::new("io.shilpo.bench-preflight").unwrap();
    runtime
        .load(&test_id, module.clone(), budget)
        .expect("preflight load must succeed");
    runtime
        .unload(&test_id)
        .expect("preflight unload must succeed");

    let mut group = c.benchmark_group("wasm/cold_load");
    group.sample_size(10);
    group.measurement_time(Duration::from_secs(10));
    group.warm_up_time(Duration::from_secs(1));

    group.bench_function("sdk_fixture", |b| {
        b.iter_custom(|iters| {
            let mut total_duration = Duration::ZERO;
            for _ in 0..iters {
                let iteration_module = module.clone();
                let start = Instant::now();
                let load_res = runtime.load(
                    black_box(&ext_id),
                    black_box(iteration_module),
                    black_box(budget),
                );
                let elapsed = start.elapsed();
                total_duration += elapsed;
                assert!(load_res.is_ok(), "load must succeed: {:?}", load_res);
                let unload_res = runtime.unload(&ext_id);
                assert!(unload_res.is_ok(), "unload must succeed: {:?}", unload_res);
            }
            total_duration
        });
    });
    group.finish();
}

criterion_group!(benches, bench_wasm_cold_load);
criterion_main!(benches);
