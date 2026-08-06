use std::{
    path::PathBuf,
    sync::Arc,
};

use shilpo_config::{HeedSessionStore, ShellConfig, ShellSessionState};

#[derive(Clone)]
pub struct SessionContext {
    pub config_path: PathBuf,
    pub active_config: ShellConfig,
    pub session_path: PathBuf,
    pub session_state: ShellSessionState,
    pub heed_store: Option<Arc<HeedSessionStore>>,
}

impl SessionContext {
    pub fn init() -> Self {
        let config_path = std::env::var("HOME")
            .map(|home| PathBuf::from(home).join(".config/shilpo/config.toml"))
            .unwrap_or_else(|_| PathBuf::from(".config/shilpo/config.toml"));
        let session_path = ShellSessionState::default_session_path();
        let heed_dir = HeedSessionStore::default_db_dir();
        Self::init_with_paths(config_path, session_path, heed_dir)
    }

    pub fn init_with_paths(
        config_path: PathBuf,
        session_path: PathBuf,
        heed_dir: PathBuf,
    ) -> Self {
        let active_config = ShellConfig::load_or_create(&config_path)
            .unwrap_or_else(|_| ShellConfig::default());
        let (session_state, _restored_fallback) =
            ShellSessionState::restore_with_fallback(&session_path);
        let heed_store = match HeedSessionStore::open_with_recovery(&heed_dir) {
            Ok(opened) => {
                if let shilpo_config::RecoveryOutcome::Quarantined { ref path } = opened.recovery {
                    tracing::warn!(
                        quarantine_path = %path.display(),
                        "LMDB session store was corrupted and has been quarantined"
                    );
                }
                Some(Arc::new(opened.store))
            }
            Err(e) => {
                tracing::warn!(
                    error = %e,
                    "LMDB session store open failed; session features running unpersisted"
                );
                None
            }
        };

        Self {
            config_path,
            active_config,
            session_path,
            session_state,
            heed_store,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_session_context_init_with_paths() {
        let temp_base = std::env::temp_dir().join(format!("shilpo_session_test_{}", uuid::Uuid::new_v4()));
        let config_path = temp_base.join("config.toml");
        let session_path = temp_base.join("session.toml");
        let heed_dir = temp_base.join("heed");

        let context = SessionContext::init_with_paths(
            config_path.clone(),
            session_path.clone(),
            heed_dir,
        );

        assert_eq!(context.config_path, config_path);
        assert_eq!(context.session_path, session_path);
        assert!(!context.session_state.dnd_active);

        let _ = std::fs::remove_dir_all(temp_base);
    }
}
