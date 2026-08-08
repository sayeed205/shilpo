use shilpo_ext_types::ExtensionId;
use std::collections::{HashMap, HashSet};
use std::time::SystemTime;

const MAX_DIAGNOSTICS: usize = 256;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DiagnosticLevel {
    Info,
    Warning,
    Error,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DiagnosticCode {
    RuntimeLoad,
    RuntimeTrap,
    RuntimeTimeout,
    FuelExhausted,
    MemoryLimit,
    InvalidOutput,
    InvalidView,
    CapabilityDenied,
    CircuitOpen,
    CircuitReset,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExtensionDiagnostic {
    pub level: DiagnosticLevel,
    pub code: DiagnosticCode,
    pub extension_id: ExtensionId,
    pub message: String,
    pub occurred_at: SystemTime,
}

pub struct CircuitBreaker {
    max_failures: u32,
    failure_counts: HashMap<ExtensionId, u32>,
    tripped_extensions: HashSet<ExtensionId>,
    diagnostics: Vec<ExtensionDiagnostic>,
}

impl Default for CircuitBreaker {
    fn default() -> Self {
        Self::new(3)
    }
}

impl CircuitBreaker {
    pub fn new(max_failures: u32) -> Self {
        Self {
            max_failures: max_failures.max(1),
            failure_counts: HashMap::new(),
            tripped_extensions: HashSet::new(),
            diagnostics: Vec::new(),
        }
    }

    pub fn record_failure(
        &mut self,
        extension_id: &ExtensionId,
        code: DiagnosticCode,
        message: impl Into<String>,
    ) -> bool {
        self.push_diagnostic(ExtensionDiagnostic {
            level: DiagnosticLevel::Error,
            code,
            extension_id: extension_id.clone(),
            message: message.into(),
            occurred_at: SystemTime::now(),
        });

        let count = self.failure_counts.entry(extension_id.clone()).or_insert(0);
        *count += 1;
        if *count < self.max_failures || self.tripped_extensions.contains(extension_id) {
            return false;
        }

        let count = *count;
        self.tripped_extensions.insert(extension_id.clone());
        self.push_diagnostic(ExtensionDiagnostic {
            level: DiagnosticLevel::Error,
            code: DiagnosticCode::CircuitOpen,
            extension_id: extension_id.clone(),
            message: format!(
                "disabled for this session after {count} consecutive extension failures"
            ),
            occurred_at: SystemTime::now(),
        });
        true
    }

    pub fn record_success(&mut self, extension_id: &ExtensionId) {
        self.failure_counts.remove(extension_id);
    }

    pub fn is_tripped(&self, extension_id: &ExtensionId) -> bool {
        self.tripped_extensions.contains(extension_id)
    }

    pub fn reset(&mut self, extension_id: &ExtensionId) {
        self.failure_counts.remove(extension_id);
        self.tripped_extensions.remove(extension_id);
        self.push_diagnostic(ExtensionDiagnostic {
            level: DiagnosticLevel::Info,
            code: DiagnosticCode::CircuitReset,
            extension_id: extension_id.clone(),
            message: "circuit breaker manually reset".to_string(),
            occurred_at: SystemTime::now(),
        });
    }

    pub fn diagnostics(&self) -> &[ExtensionDiagnostic] {
        &self.diagnostics
    }

    pub fn clear_diagnostics(&mut self) {
        self.diagnostics.clear();
    }

    fn push_diagnostic(&mut self, diagnostic: ExtensionDiagnostic) {
        if self.diagnostics.len() == MAX_DIAGNOSTICS {
            self.diagnostics.remove(0);
        }
        self.diagnostics.push(diagnostic);
    }
}
