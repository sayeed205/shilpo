use std::collections::HashMap;
use std::sync::Arc;

use super::{
    CompositorAdapter, GenericWaylandCompositorBackend, HyprlandCompositorBackend,
    NiriCompositorService, NullCompositorBackend,
    detect::{self, CompositorKind},
};

pub type BackendFactory = Box<dyn Fn() -> Option<Arc<dyn CompositorAdapter>> + Send + Sync>;

pub struct CandidateBackend {
    pub name: &'static str,
    pub factory: BackendFactory,
}

/// Registry mapping `CompositorKind` to an ordered candidate chain of backend factories.
pub struct CompositorRegistry {
    candidates: HashMap<CompositorKind, Vec<CandidateBackend>>,
}

impl Default for CompositorRegistry {
    fn default() -> Self {
        Self::default_registry()
    }
}

impl CompositorRegistry {
    /// Creates an empty registry with no candidates configured.
    pub fn empty() -> Self {
        Self {
            candidates: HashMap::new(),
        }
    }

    /// Creates the standard production registry with built-in backends.
    pub fn default_registry() -> Self {
        let mut registry = Self::empty();
        registry.register(
            CompositorKind::Niri,
            "niri",
            Box::new(|| Some(NiriCompositorService::new())),
        );
        registry.register(
            CompositorKind::Niri,
            "generic",
            Box::new(|| Some(GenericWaylandCompositorBackend::new())),
        );
        registry.register(
            CompositorKind::Hyprland,
            "hyprland",
            Box::new(|| Some(HyprlandCompositorBackend::new())),
        );
        registry.register(
            CompositorKind::Hyprland,
            "generic",
            Box::new(|| Some(GenericWaylandCompositorBackend::new())),
        );
        for kind in [
            CompositorKind::Sway,
            CompositorKind::Labwc,
            CompositorKind::Dwl,
            CompositorKind::River,
            CompositorKind::Kde,
            CompositorKind::Unknown,
        ] {
            registry.register(
                kind,
                "generic",
                Box::new(|| Some(GenericWaylandCompositorBackend::new())),
            );
        }
        registry
    }

    /// Registers a backend factory for a compositor kind.
    pub fn register(&mut self, kind: CompositorKind, name: &'static str, factory: BackendFactory) {
        self.candidates
            .entry(kind)
            .or_default()
            .push(CandidateBackend { name, factory });
    }

    /// Tries candidates in order for the given kind and returns the first successfully constructed backend.
    /// Falls back to `NullCompositorBackend` if no candidates succeed.
    pub fn select_backend(
        &self,
        kind: CompositorKind,
    ) -> (Arc<dyn CompositorAdapter>, &'static str) {
        if let Some(chain) = self.candidates.get(&kind) {
            for candidate in chain {
                if let Some(adapter) = (candidate.factory)() {
                    return (adapter, candidate.name);
                }
            }
        }
        (NullCompositorBackend::new(), "null")
    }

    /// Selects backend using an injected environment variable lookup.
    pub fn select_backend_for_env(
        &self,
        get_var: &dyn Fn(&str) -> Option<String>,
    ) -> (Arc<dyn CompositorAdapter>, CompositorKind, &'static str) {
        let kind = detect::detect_from(get_var);
        let (adapter, name) = self.select_backend(kind);
        (adapter, kind, name)
    }
}

/// Detects the active compositor, selects the best matching backend, logs the selection,
/// and returns the configured adapter.
pub fn init_compositor() -> Arc<dyn CompositorAdapter> {
    init_compositor_with(&CompositorRegistry::default_registry())
}

/// Detects the active compositor and selects a backend using the provided registry.
pub fn init_compositor_with(registry: &CompositorRegistry) -> Arc<dyn CompositorAdapter> {
    let kind = detect::detect();
    let (adapter, name) = registry.select_backend(kind);
    tracing::info!(
        detected_compositor = %kind,
        selected_backend = name,
        "compositor backend initialized"
    );
    adapter
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compositor::{CompositorSnapshot, DomainLifecycle, TestCompositorAdapter};

    #[test]
    fn test_registry_selects_expected_candidate_in_order() {
        let mut registry = CompositorRegistry::empty();

        // Register a failing candidate first, then a succeeding candidate
        registry.register(CompositorKind::Niri, "failing_niri", Box::new(|| None));
        registry.register(
            CompositorKind::Niri,
            "test_niri",
            Box::new(|| Some(Arc::new(TestCompositorAdapter::new_default()))),
        );

        let (adapter, name) = registry.select_backend(CompositorKind::Niri);
        assert_eq!(name, "test_niri");
        assert_eq!(adapter.current().connection, DomainLifecycle::Unavailable);
    }

    #[test]
    fn test_registry_falls_back_to_null_backend_when_all_candidates_fail() {
        let mut registry = CompositorRegistry::empty();
        registry.register(
            CompositorKind::Hyprland,
            "failing_hyprland",
            Box::new(|| None),
        );

        let (adapter, name) = registry.select_backend(CompositorKind::Hyprland);
        assert_eq!(name, "null");
        assert_eq!(adapter.current().connection, DomainLifecycle::Unavailable);
    }

    #[test]
    fn test_registry_select_backend_for_env() {
        let mut registry = CompositorRegistry::empty();
        registry.register(
            CompositorKind::Sway,
            "mock_sway",
            Box::new(|| {
                let snapshot = CompositorSnapshot {
                    connection: DomainLifecycle::Ready,
                    ..Default::default()
                };
                Some(Arc::new(TestCompositorAdapter::new(snapshot)))
            }),
        );

        let env = |key: &str| {
            if key == "SWAYSOCK" {
                Some("/run/user/1000/sway-ipc.sock".to_string())
            } else {
                None
            }
        };

        let (adapter, kind, name) = registry.select_backend_for_env(&env);
        assert_eq!(kind, CompositorKind::Sway);
        assert_eq!(name, "mock_sway");
        assert_eq!(adapter.current().connection, DomainLifecycle::Ready);
    }

    #[test]
    fn test_default_registry_routes_sway_and_generic_kinds_to_generic() {
        let registry = CompositorRegistry::default_registry();
        for kind in [
            CompositorKind::Sway,
            CompositorKind::Labwc,
            CompositorKind::Dwl,
            CompositorKind::River,
            CompositorKind::Kde,
            CompositorKind::Unknown,
        ] {
            let (_, name) = registry.select_backend(kind);
            assert_eq!(name, "generic", "failed for {kind:?}");
        }
    }

    #[test]
    fn test_default_registry_routes_niri_to_niri_first() {
        let registry = CompositorRegistry::default_registry();
        let (_, name) = registry.select_backend(CompositorKind::Niri);
        assert_eq!(name, "niri");
    }

    #[test]
    fn test_default_registry_routes_hyprland_to_hyprland_first() {
        let registry = CompositorRegistry::default_registry();
        let (_, name) = registry.select_backend(CompositorKind::Hyprland);
        assert_eq!(name, "hyprland");
    }

    #[test]
    fn test_sway_falls_back_to_null_when_all_candidates_fail() {
        let mut registry = CompositorRegistry::empty();
        registry.register(CompositorKind::Sway, "failing_generic", Box::new(|| None));

        let (adapter, name) = registry.select_backend(CompositorKind::Sway);
        assert_eq!(name, "null");
        assert_eq!(adapter.current().connection, DomainLifecycle::Unavailable);
    }
}
