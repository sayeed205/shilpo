use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Component, Path, PathBuf};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use flate2::read::GzDecoder;
use schemars::JsonSchema;
use semver::Version;
use serde::{Deserialize, Serialize};
use shilpo_ext_api::{Capability, ExtensionId, ExtensionManifest, SUPPORTED_API_VERSION};
use shilpo_registry_contract::ContractError;
pub use shilpo_registry_contract::{
    KeyRotationDelegation, PackageSignature, REGISTRY_SCHEMA_VERSION, RegistryIndex,
    RegistryRelease, RegistrySource, ReleaseChannel, SignedRegistryIndex, capabilities_hash,
    decode_key_pair, generate_signing_key, hash_bytes, hash_file, package_signature_path,
    package_signing_message, public_key_fingerprint, release_signing_payload, rotation_message,
    sign_package, sign_registry_index, sign_release, validate_source, verify_registry_index,
    verify_release_signature, verify_signature,
};
use tar::Archive;

use crate::cli::ExtensionCli;

const MAX_PACKAGE_BYTES: u64 = 64 * 1024 * 1024;
const MAX_INDEX_BYTES: usize = 8 * 1024 * 1024;
const RECEIPT_SCHEMA_VERSION: u32 = 1;
pub const CURRENT_SHILPO_VERSION: &str = "0.1.0";
const OFFICIAL_SOURCE_ID: &str = "shilpo";
const OFFICIAL_SOURCE_NAME: &str = "Shilpo Extensions";
const OFFICIAL_SOURCE_URL: &str = "https://extensions.shilpo.org/index.json";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CatalogPaths {
    pub data_dir: PathBuf,
    pub config_dir: PathBuf,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum SecretPolicy {
    #[default]
    Retain,
    Delete,
}

impl CatalogPaths {
    pub fn new(data_dir: impl Into<PathBuf>, config_dir: impl Into<PathBuf>) -> Self {
        Self {
            data_dir: data_dir.into(),
            config_dir: config_dir.into(),
        }
    }

    pub fn platform_default() -> Self {
        Self::new(default_extension_data_dir(), default_extension_config_dir())
    }

    /// Directory containing the dedicated extension-scoped operational state store.
    pub fn state_store_dir(&self) -> PathBuf {
        self.data_dir.join("extensions").join("state.lmdb")
    }

    fn extensions_data(&self) -> PathBuf {
        self.data_dir.join("extensions")
    }

    fn installed_dir(&self) -> PathBuf {
        self.extensions_data().join("installed")
    }

    fn staging_dir(&self) -> PathBuf {
        self.extensions_data().join("staging")
    }

    fn receipts_dir(&self) -> PathBuf {
        self.extensions_data().join("receipts")
    }

    fn indexes_dir(&self) -> PathBuf {
        self.extensions_data().join("indexes")
    }

    fn extensions_config(&self) -> PathBuf {
        self.config_dir.join("extensions")
    }

    fn grants_dir(&self) -> PathBuf {
        self.extensions_config().join("grants")
    }

    fn sources_path(&self) -> PathBuf {
        self.extensions_config().join("sources.toml")
    }
}

impl Default for CatalogPaths {
    fn default() -> Self {
        Self::platform_default()
    }
}

pub fn default_extension_data_dir() -> PathBuf {
    xdg_dir("XDG_DATA_HOME", ".local/share").join("shilpo")
}

pub fn default_extension_config_dir() -> PathBuf {
    xdg_dir("XDG_CONFIG_HOME", ".config").join("shilpo")
}

fn xdg_dir(variable: &str, fallback: &str) -> PathBuf {
    std::env::var_os(variable).map_or_else(
        || {
            std::env::var_os("HOME").map_or_else(
                || PathBuf::from(fallback),
                |home| PathBuf::from(home).join(fallback),
            )
        },
        PathBuf::from,
    )
}

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum TrustState {
    Official,
    VerifiedPublisher,
    SignedThirdParty,
    #[default]
    Unverified,
}

impl fmt::Display for TrustState {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let value = match self {
            Self::Official => "official",
            Self::VerifiedPublisher => "verified publisher",
            Self::SignedThirdParty => "signed third-party",
            Self::Unverified => "unverified",
        };
        formatter.write_str(value)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct InstallationReceipt {
    #[serde(default = "receipt_schema_version")]
    pub schema_version: u32,
    pub id: ExtensionId,
    #[serde(default)]
    pub selected_channel: ReleaseChannel,
    pub active: InstalledVersionReceipt,
    pub previous: Option<InstalledVersionReceipt>,
    pub pending: Option<InstalledVersionReceipt>,
    #[serde(default)]
    pub last_update_failure: Option<String>,
    #[serde(default)]
    pub rollback_active: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct InstalledVersionReceipt {
    pub version: Version,
    pub source: String,
    #[serde(default)]
    pub source_id: Option<String>,
    #[serde(default)]
    pub source_key: Option<String>,
    pub publisher: Option<String>,
    pub publisher_key: Option<String>,
    pub publisher_public_key: Option<String>,
    pub package_hash: String,
    pub trust: TrustState,
    pub channel: ReleaseChannel,
    pub installed_at_unix_seconds: u64,
}

fn receipt_schema_version() -> u32 {
    RECEIPT_SCHEMA_VERSION
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct StoredGrants {
    pub extension_id: ExtensionId,
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub granted_capabilities: Vec<Capability>,
}

impl StoredGrants {
    pub fn disabled(extension_id: ExtensionId) -> Self {
        Self {
            extension_id,
            enabled: false,
            granted_capabilities: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct StoredSources {
    #[serde(default)]
    source: Vec<RegistrySource>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct CatalogExtension {
    pub release: RegistryRelease,
    pub source: RegistrySource,
    pub trust: TrustState,
    pub publisher_conflict: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct InstalledExtension {
    pub receipt: InstallationReceipt,
    pub manifest: ExtensionManifest,
    pub grants: StoredGrants,
    pub package_dir: PathBuf,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum UpdateState {
    UpToDate,
    Available,
    AwaitingPermissionReview,
    Incompatible,
    PublisherConflict,
    Yanked,
    FailedUsingPreviousVersion,
    RollbackActive,
    DevelopmentOverrideActive,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ExtensionUpdate {
    pub id: ExtensionId,
    pub installed_version: Version,
    pub available: Option<CatalogExtension>,
    pub state: UpdateState,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct ExtensionCatalogSnapshot {
    pub discover: Vec<CatalogExtension>,
    pub installed: Vec<InstalledExtension>,
    pub updates: Vec<ExtensionUpdate>,
    pub sources: Vec<RegistrySource>,
    pub diagnostics: Vec<String>,
}

#[derive(Debug)]
pub enum CatalogError {
    Io(String),
    InvalidPackage(String),
    InvalidSignature(String),
    InvalidRegistry(String),
    NotFound(String),
    PublisherConflict(String),
    PermissionReviewRequired(String),
}

impl fmt::Display for CatalogError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(message) => write!(formatter, "I/O error: {message}"),
            Self::InvalidPackage(message) => write!(formatter, "invalid package: {message}"),
            Self::InvalidSignature(message) => write!(formatter, "invalid signature: {message}"),
            Self::InvalidRegistry(message) => write!(formatter, "invalid registry: {message}"),
            Self::NotFound(message) => write!(formatter, "not found: {message}"),
            Self::PublisherConflict(message) => write!(formatter, "publisher conflict: {message}"),
            Self::PermissionReviewRequired(message) => {
                write!(formatter, "permission review required: {message}")
            }
        }
    }
}

impl std::error::Error for CatalogError {}

impl From<ContractError> for CatalogError {
    fn from(error: ContractError) -> Self {
        match error {
            ContractError::Io(message) => Self::Io(message),
            ContractError::InvalidSignature(message) => Self::InvalidSignature(message),
            ContractError::InvalidRegistry(message) => Self::InvalidRegistry(message),
        }
    }
}

#[derive(Clone, Debug)]
pub struct ExtensionCatalog {
    paths: CatalogPaths,
    shilpo_version: Version,
}

impl ExtensionCatalog {
    pub fn open(paths: CatalogPaths, shilpo_version: Version) -> Self {
        Self {
            paths,
            shilpo_version,
        }
    }

    pub fn open_default() -> Self {
        Self::open(
            CatalogPaths::platform_default(),
            Version::parse(CURRENT_SHILPO_VERSION).expect("Shilpo version is valid semver"),
        )
    }

    pub fn paths(&self) -> &CatalogPaths {
        &self.paths
    }

    pub fn snapshot(&self) -> ExtensionCatalogSnapshot {
        let mut snapshot = ExtensionCatalogSnapshot::default();
        snapshot.sources = match self.sources() {
            Ok(sources) => sources,
            Err(error) => {
                snapshot.diagnostics.push(error.to_string());
                Vec::new()
            }
        };
        snapshot.installed = match self.installed() {
            Ok(installed) => installed,
            Err(error) => {
                snapshot.diagnostics.push(error.to_string());
                Vec::new()
            }
        };
        let releases = self.catalog_releases(&snapshot.sources, &mut snapshot.diagnostics);
        snapshot.discover = self.discover(&releases);
        snapshot.updates = self.resolve_updates(&snapshot.installed, &releases);
        snapshot
    }

    pub fn snapshot_with_development(
        &self,
        development_ids: impl IntoIterator<Item = ExtensionId>,
    ) -> ExtensionCatalogSnapshot {
        let development_ids = development_ids.into_iter().collect::<BTreeSet<_>>();
        let mut snapshot = self.snapshot();
        for update in &mut snapshot.updates {
            if development_ids.contains(&update.id) {
                update.state = UpdateState::DevelopmentOverrideActive;
            }
        }
        snapshot
    }

    pub fn install_local(&self, package: &Path) -> Result<InstallationReceipt, CatalogError> {
        let signature_path = package_signature_path(package);
        let signature = if signature_path.is_file() {
            let source = fs::read_to_string(&signature_path)
                .map_err(|error| io_error(&signature_path, error))?;
            Some(
                serde_json::from_str::<PackageSignature>(&source)
                    .map_err(|error| CatalogError::InvalidSignature(error.to_string()))?,
            )
        } else {
            None
        };
        self.install_package(package, PackageProvenance::Local(signature))
    }

    pub fn install_url(
        &self,
        url: &str,
        expected_hash: Option<&str>,
    ) -> Result<InstallationReceipt, CatalogError> {
        if !url.starts_with("https://") {
            return Err(CatalogError::InvalidPackage(
                "direct remote packages must use HTTPS".into(),
            ));
        }
        let package = fetch_to_staging(url, &self.paths.staging_dir(), MAX_PACKAGE_BYTES)?;
        let signature_url = format!("{url}.sig.json");
        let signature = fetch_bytes(&signature_url, 64 * 1024)
            .ok()
            .and_then(|bytes| serde_json::from_slice::<PackageSignature>(&bytes).ok());
        let actual_hash = hash_file(&package)?;
        if let Some(expected_hash) = expected_hash
            && actual_hash != expected_hash
        {
            let _ = fs::remove_file(&package);
            return Err(CatalogError::InvalidSignature(
                "downloaded package does not match --hash".into(),
            ));
        }
        if signature.is_none() && expected_hash.is_none() {
            let _ = fs::remove_file(&package);
            return Err(CatalogError::InvalidSignature(
                "direct URL installation requires --hash or a valid signature sidecar".into(),
            ));
        }
        let result = self.install_package(
            &package,
            PackageProvenance::Direct(url.to_owned(), signature),
        );
        let _ = fs::remove_file(package);
        result
    }

    pub fn install_from_catalog(
        &self,
        extension_id: &ExtensionId,
    ) -> Result<InstallationReceipt, CatalogError> {
        let snapshot = self.snapshot();
        let candidate = snapshot
            .updates
            .into_iter()
            .find(|update| update.id == *extension_id && update.state == UpdateState::Available)
            .and_then(|update| update.available)
            .or_else(|| {
                snapshot
                    .discover
                    .into_iter()
                    .find(|entry| entry.release.id == *extension_id && !entry.publisher_conflict)
            })
            .ok_or_else(|| CatalogError::NotFound(extension_id.to_string()))?;
        let package = fetch_to_staging(
            &candidate.release.package_url,
            &self.paths.staging_dir(),
            MAX_PACKAGE_BYTES,
        )?;
        let result =
            self.install_package(&package, PackageProvenance::Registry(Box::new(candidate)));
        let _ = fs::remove_file(package);
        if let Err(error) = &result {
            let _ = self.record_update_failure(extension_id, error);
        }
        result
    }

    pub fn set_enabled(
        &self,
        extension_id: &ExtensionId,
        enabled: bool,
    ) -> Result<(), CatalogError> {
        self.receipt(extension_id)?;
        let mut grants = self.load_grants(extension_id)?;
        grants.enabled = enabled;
        self.save_grants(&grants)
    }

    pub fn set_channel(
        &self,
        extension_id: &ExtensionId,
        channel: ReleaseChannel,
    ) -> Result<(), CatalogError> {
        let mut receipt = self.receipt(extension_id)?;
        receipt.selected_channel = channel;
        self.save_receipt(&receipt)
    }

    pub fn approve_pending(
        &self,
        extension_id: &ExtensionId,
        granted_capabilities: Vec<Capability>,
    ) -> Result<InstallationReceipt, CatalogError> {
        let mut receipt = self.receipt(extension_id)?;
        let pending = receipt.pending.clone().ok_or_else(|| {
            CatalogError::NotFound(format!("extension '{extension_id}' has no pending update"))
        })?;
        let mut grants = self.load_grants(extension_id)?;
        grants.granted_capabilities = granted_capabilities;
        self.save_grants(&grants)?;
        receipt.previous = Some(receipt.active);
        receipt.active = pending;
        receipt.pending = None;
        receipt.last_update_failure = None;
        receipt.rollback_active = false;
        self.save_receipt(&receipt)?;
        Ok(receipt)
    }

    pub fn approve_capabilities(
        &self,
        extension_id: &ExtensionId,
        granted_capabilities: Vec<Capability>,
    ) -> Result<StoredGrants, CatalogError> {
        let requested = self.requested_capabilities(extension_id)?;
        if granted_capabilities
            .iter()
            .any(|capability| !requested.contains(capability))
        {
            return Err(CatalogError::InvalidPackage(
                "a grant is not declared by the active extension manifest".into(),
            ));
        }
        let mut grants = self.load_grants(extension_id)?;
        grants.granted_capabilities = granted_capabilities;
        self.save_grants(&grants)?;
        Ok(grants)
    }

    pub fn requested_capabilities(
        &self,
        extension_id: &ExtensionId,
    ) -> Result<Vec<Capability>, CatalogError> {
        let receipt = self.receipt(extension_id)?;
        self.capabilities_for_version(extension_id, &receipt.active.version)
    }

    pub fn pending_capabilities(
        &self,
        extension_id: &ExtensionId,
    ) -> Result<Vec<Capability>, CatalogError> {
        let receipt = self.receipt(extension_id)?;
        let pending = receipt.pending.ok_or_else(|| {
            CatalogError::NotFound(format!("extension '{extension_id}' has no pending update"))
        })?;
        self.capabilities_for_version(extension_id, &pending.version)
    }

    fn capabilities_for_version(
        &self,
        extension_id: &ExtensionId,
        version: &Version,
    ) -> Result<Vec<Capability>, CatalogError> {
        let manifest_path = self
            .package_dir(extension_id, version)
            .join("extension.toml");
        let source =
            fs::read_to_string(&manifest_path).map_err(|error| io_error(&manifest_path, error))?;
        ExtensionManifest::from_toml(&source)
            .map(|manifest| manifest.capabilities)
            .map_err(|error| CatalogError::InvalidPackage(error.to_string()))
    }

    pub fn rollback(
        &self,
        extension_id: &ExtensionId,
    ) -> Result<InstallationReceipt, CatalogError> {
        let mut receipt = self.receipt(extension_id)?;
        let previous = receipt.previous.clone().ok_or_else(|| {
            CatalogError::NotFound(format!(
                "extension '{extension_id}' has no rollback version"
            ))
        })?;
        if !self.package_dir(extension_id, &previous.version).is_dir() {
            return Err(CatalogError::NotFound(format!(
                "rollback package for '{extension_id}' is missing"
            )));
        }
        receipt.previous = Some(receipt.active);
        receipt.active = previous;
        receipt.pending = None;
        receipt.last_update_failure = None;
        receipt.rollback_active = true;
        self.save_receipt(&receipt)?;
        Ok(receipt)
    }
    pub fn uninstall(&self, extension_id: &ExtensionId) -> Result<(), CatalogError> {
        self.uninstall_with_policies(
            extension_id,
            SecretPolicy::Retain,
            None,
            crate::state::StatePolicy::Retain,
            None,
            Instant::now() + Duration::from_secs(30),
        )
    }

    pub fn uninstall_with_secrets_policy(
        &self,
        extension_id: &ExtensionId,
        secret_policy: SecretPolicy,
        broker: Option<&dyn crate::secrets::SecretBroker>,
    ) -> Result<(), CatalogError> {
        self.uninstall_with_policies(
            extension_id,
            secret_policy,
            broker,
            crate::state::StatePolicy::Retain,
            None,
            Instant::now() + Duration::from_secs(30),
        )
    }

    pub fn uninstall_with_policies(
        &self,
        extension_id: &ExtensionId,
        secret_policy: SecretPolicy,
        broker: Option<&dyn crate::secrets::SecretBroker>,
        state_policy: crate::state::StatePolicy,
        state_store: Option<&dyn crate::state::StateStore>,
        deadline: Instant,
    ) -> Result<(), CatalogError> {
        let receipt_path = self.receipt_path(extension_id);
        if !receipt_path.is_file() {
            return Err(CatalogError::NotFound(extension_id.to_string()));
        }

        if let (SecretPolicy::Delete, Some(broker)) = (secret_policy, broker) {
            broker.delete_all(extension_id, deadline).map_err(|error| {
                CatalogError::Io(format!(
                    "failed to delete extension secrets for {extension_id}: {error}"
                ))
            })?;
        }

        let package_dir = self.extension_dir(extension_id);
        let grants_path = self.grants_path(extension_id);
        let trash_dir = self.paths.staging_dir().join(format!(
            "uninstall-{}-{}",
            extension_id,
            unique_suffix()
        ));
        fs::create_dir_all(&trash_dir).map_err(|error| io_error(&trash_dir, error))?;

        let staged_package = trash_dir.join("installed");
        let staged_receipt = trash_dir.join("receipt.toml");
        let staged_grants = trash_dir.join("grants.toml");

        if package_dir.exists() {
            fs::rename(&package_dir, &staged_package)
                .map_err(|error| io_error(&package_dir, error))?;
        }
        if receipt_path.exists() {
            let res = fs::rename(&receipt_path, &staged_receipt);
            if let Err(error) = res {
                if staged_package.exists() {
                    let _ = fs::rename(&staged_package, &package_dir);
                }
                let _ = fs::remove_dir_all(&trash_dir);
                return Err(io_error(&receipt_path, error));
            }
        }
        if grants_path.exists() {
            let _ = fs::rename(&grants_path, &staged_grants);
        }

        let should_delete_state = state_policy == crate::state::StatePolicy::Delete;
        if should_delete_state {
            let state_result = match state_store {
                Some(store) => store.delete_all(extension_id),
                None => Err(crate::state::StateStoreError::BackendUnavailable(
                    "extension state store is unavailable; refusing destructive uninstall".into(),
                )),
            };
            if let Err(error) = state_result {
                if staged_package.exists() {
                    let _ = fs::rename(&staged_package, &package_dir);
                }
                if staged_receipt.exists() {
                    let _ = fs::rename(&staged_receipt, &receipt_path);
                }
                if staged_grants.exists() {
                    let _ = fs::rename(&staged_grants, &grants_path);
                }
                let _ = fs::remove_dir_all(&trash_dir);
                return Err(CatalogError::Io(format!(
                    "failed to delete extension state for {extension_id}: {error}"
                )));
            }
        }

        let _ = fs::remove_dir_all(&trash_dir);
        Ok(())
    }

    pub fn add_source(&self, source: RegistrySource) -> Result<(), CatalogError> {
        if source.official {
            return Err(CatalogError::InvalidRegistry(
                "official trust is reserved for Shilpo's build-time registry root".into(),
            ));
        }
        validate_source(&source)?;
        let mut sources = self.sources()?;
        if sources.iter().any(|item| item.id == source.id) {
            return Err(CatalogError::InvalidRegistry(format!(
                "source ID '{}' is already configured",
                source.id
            )));
        }
        sources.push(source);
        sources.sort_by(|left, right| left.id.cmp(&right.id));
        self.save_sources(&sources)
    }

    pub fn remove_source(&self, source_id: &str) -> Result<(), CatalogError> {
        let mut sources = self.sources()?;
        let before = sources.len();
        sources.retain(|source| source.id != source_id || source.official);
        if before == sources.len() {
            return Err(CatalogError::NotFound(source_id.to_owned()));
        }
        self.save_sources(&sources)?;
        let cache = self.index_path(source_id);
        if cache.exists() {
            fs::remove_file(&cache).map_err(|error| io_error(&cache, error))?;
        }
        Ok(())
    }

    pub fn refresh_sources(&self) -> Result<Vec<String>, CatalogError> {
        let sources = self.sources()?;
        let mut diagnostics = Vec::new();
        for source in sources.into_iter().filter(|source| source.enabled) {
            match fetch_bytes(&source.index_url, MAX_INDEX_BYTES as u64)
                .and_then(|bytes| self.store_verified_index(&source, &bytes))
            {
                Ok(()) => diagnostics.push(format!("refreshed source '{}'", source.id)),
                Err(error) => diagnostics.push(format!("source '{}': {error}", source.id)),
            }
        }
        Ok(diagnostics)
    }

    pub fn store_index_bytes(&self, source_id: &str, bytes: &[u8]) -> Result<(), CatalogError> {
        let source = self
            .sources()?
            .into_iter()
            .find(|source| source.id == source_id)
            .ok_or_else(|| CatalogError::NotFound(source_id.to_owned()))?;
        self.store_verified_index(&source, bytes)
    }

    pub fn receipt(&self, extension_id: &ExtensionId) -> Result<InstallationReceipt, CatalogError> {
        let path = self.receipt_path(extension_id);
        let source = fs::read_to_string(&path).map_err(|error| io_error(&path, error))?;
        let receipt: InstallationReceipt = toml::from_str(&source)
            .map_err(|error| CatalogError::InvalidPackage(error.to_string()))?;
        if receipt.schema_version != RECEIPT_SCHEMA_VERSION || receipt.id != *extension_id {
            return Err(CatalogError::InvalidPackage(format!(
                "receipt identity or schema mismatch at {}",
                path.display()
            )));
        }
        Ok(receipt)
    }

    pub fn load_grants(&self, extension_id: &ExtensionId) -> Result<StoredGrants, CatalogError> {
        let path = self.grants_path(extension_id);
        if !path.exists() {
            return Ok(StoredGrants::disabled(extension_id.clone()));
        }
        let source = fs::read_to_string(&path).map_err(|error| io_error(&path, error))?;
        let grants: StoredGrants = toml::from_str(&source)
            .map_err(|error| CatalogError::InvalidPackage(error.to_string()))?;
        if grants.extension_id != *extension_id {
            return Err(CatalogError::InvalidPackage(format!(
                "grant identity mismatch at {}",
                path.display()
            )));
        }
        Ok(grants)
    }

    pub fn active_packages(&self) -> Result<Vec<InstalledExtension>, CatalogError> {
        self.installed().map(|installed| {
            installed
                .into_iter()
                .filter(|extension| extension.grants.enabled)
                .collect()
        })
    }

    pub fn switch_source(
        &self,
        extension_id: &ExtensionId,
        target_source_id: &str,
    ) -> Result<InstallationReceipt, CatalogError> {
        self.switch_source_with_policies(
            extension_id,
            target_source_id,
            SecretPolicy::Delete,
            None,
            Instant::now() + Duration::from_secs(30),
        )
    }

    pub fn switch_source_with_secrets_policy(
        &self,
        extension_id: &ExtensionId,
        target_source_id: &str,
        secret_policy: SecretPolicy,
        broker: Option<&dyn crate::secrets::SecretBroker>,
    ) -> Result<InstallationReceipt, CatalogError> {
        self.switch_source_with_policies(
            extension_id,
            target_source_id,
            secret_policy,
            broker,
            Instant::now() + Duration::from_secs(30),
        )
    }

    pub fn switch_source_with_policies(
        &self,
        extension_id: &ExtensionId,
        target_source_id: &str,
        secret_policy: SecretPolicy,
        broker: Option<&dyn crate::secrets::SecretBroker>,
        deadline: Instant,
    ) -> Result<InstallationReceipt, CatalogError> {
        let current_receipt = self.receipt(extension_id)?;
        let sources = self.sources()?;
        let target_source = sources
            .into_iter()
            .find(|source| source.id == target_source_id && source.enabled)
            .ok_or_else(|| {
                CatalogError::NotFound(format!("source '{target_source_id}' not found or disabled"))
            })?;
        let index = self.load_verified_index(&target_source)?.ok_or_else(|| {
            CatalogError::NotFound(format!(
                "index for source '{target_source_id}' not found; run refresh first"
            ))
        })?;
        let mut candidates = index
            .index
            .releases
            .into_iter()
            .filter(|release| {
                release.id == *extension_id
                    && release.channel == current_receipt.selected_channel
                    && !release.yanked
                    && release.min_shilpo_version <= self.shilpo_version
                    && release.api_version
                        == Version::parse(SUPPORTED_API_VERSION)
                            .expect("supported API version is valid")
            })
            .collect::<Vec<_>>();
        candidates.sort_by(|left, right| right.version.cmp(&left.version));
        let release = candidates.into_iter().next().ok_or_else(|| {
            CatalogError::NotFound(format!(
                "no compatible release for '{extension_id}' in source '{target_source_id}'"
            ))
        })?;

        let trust = trust_for_release(&target_source, &release);
        let target_fingerprint = public_key_fingerprint(&release.publisher_public_key)?;
        let publisher_changed =
            current_receipt.active.publisher_key.as_deref() != Some(&target_fingerprint);

        let target_catalog_ext = CatalogExtension {
            release,
            source: target_source,
            trust,
            publisher_conflict: false,
        };

        let package = fetch_to_staging(
            &target_catalog_ext.release.package_url,
            &self.paths.staging_dir(),
            MAX_PACKAGE_BYTES,
        )?;

        let result = self.install_package_internal(
            &package,
            PackageProvenance::Registry(Box::new(target_catalog_ext)),
            true,
        );
        let _ = fs::remove_file(package);

        // Secrets and grants must only be reset once the switch has actually succeeded —
        // install_package_internal has already verified the package hash, provenance
        // signature, manifest, and runtime activation by the time it returns Ok. Doing
        // this before that point would destroy a working extension's secrets on a merely
        // attempted (and failed) switch. The grants reset itself lives inside
        // install_package_internal, gated on the same `publisher_changed` condition, since
        // it needs the freshly-parsed manifest id; only secret deletion needs to happen
        // out here.
        if result.is_ok()
            && publisher_changed
            && secret_policy == SecretPolicy::Delete
            && let Some(broker) = broker
        {
            let _ = broker.delete_all(extension_id, deadline);
        }

        result
    }

    fn install_package(
        &self,
        package: &Path,
        provenance: PackageProvenance,
    ) -> Result<InstallationReceipt, CatalogError> {
        self.install_package_internal(package, provenance, false)
    }

    fn install_package_internal(
        &self,
        package: &Path,
        provenance: PackageProvenance,
        is_explicit_source_switch: bool,
    ) -> Result<InstallationReceipt, CatalogError> {
        let metadata = fs::metadata(package).map_err(|error| io_error(package, error))?;
        if !metadata.is_file() || metadata.len() > MAX_PACKAGE_BYTES {
            return Err(CatalogError::InvalidPackage(
                "package is missing or exceeds the 64 MiB limit".into(),
            ));
        }
        let package_hash = hash_file(package)?;
        let verified = provenance.verify(&package_hash)?;
        let stage_parent = self.paths.staging_dir();
        fs::create_dir_all(&stage_parent).map_err(|error| io_error(&stage_parent, error))?;
        let stage = stage_parent.join(format!("install-{}", unique_suffix()));
        fs::create_dir(&stage).map_err(|error| io_error(&stage, error))?;
        let install_result = (|| {
            extract_package(package, &stage)?;
            let check = ExtensionCli::check(&stage);
            if !check.success {
                return Err(CatalogError::InvalidPackage(check.diagnostics.join("; ")));
            }
            let manifest_path = stage.join("extension.toml");
            let manifest_source = fs::read_to_string(&manifest_path)
                .map_err(|error| io_error(&manifest_path, error))?;
            let manifest = ExtensionManifest::from_toml(&manifest_source)
                .map_err(|error| CatalogError::InvalidPackage(error.to_string()))?;
            if manifest.min_shilpo_version > self.shilpo_version {
                return Err(CatalogError::InvalidPackage(format!(
                    "requires Shilpo {}, running {}",
                    manifest.min_shilpo_version, self.shilpo_version
                )));
            }
            crate::cli::probe_runtime(&stage, &manifest).map_err(|error| {
                CatalogError::InvalidPackage(format!("runtime activation probe failed: {error}"))
            })?;
            if let Some(expected) = &verified.expected_release {
                verify_manifest_against_release(&manifest, expected)?;
            }
            let previous_receipt = self.receipt(&manifest.id).ok();
            if !is_explicit_source_switch {
                verify_provenance_continuity(previous_receipt.as_ref(), &verified)?;
            }
            let target = self.package_dir(&manifest.id, &manifest.version);
            if target.exists() {
                if is_explicit_source_switch {
                    let _ = make_tree_writable(&target);
                    let _ = fs::remove_dir_all(&target);
                    let parent = target
                        .parent()
                        .expect("version directory always has a parent");
                    fs::create_dir_all(parent).map_err(|error| io_error(parent, error))?;
                    mark_files_read_only(&stage)?;
                    fs::rename(&stage, &target).map_err(|error| io_error(&target, error))?;
                } else {
                    let existing_hash = previous_receipt
                        .as_ref()
                        .filter(|receipt| receipt.active.version == manifest.version)
                        .map(|receipt| receipt.active.package_hash.as_str());
                    if existing_hash != Some(package_hash.as_str()) {
                        return Err(CatalogError::InvalidPackage(format!(
                            "{} {} is immutable and already exists",
                            manifest.id, manifest.version
                        )));
                    }
                    fs::remove_dir_all(&stage).map_err(|error| io_error(&stage, error))?;
                }
            } else {
                let parent = target
                    .parent()
                    .expect("version directory always has a parent");
                fs::create_dir_all(parent).map_err(|error| io_error(parent, error))?;
                mark_files_read_only(&stage)?;
                fs::rename(&stage, &target).map_err(|error| io_error(&target, error))?;
            }

            let mut grants = self.load_grants(&manifest.id)?;
            let publisher_changed = previous_receipt.as_ref().is_some_and(|r| {
                r.active.publisher_key.as_deref() != verified.publisher_key.as_deref()
            });
            if is_explicit_source_switch && publisher_changed {
                grants = StoredGrants::disabled(manifest.id.clone());
                self.save_grants(&grants)?;
            }

            let capability_expansion = previous_receipt.is_some()
                && !publisher_changed
                && manifest
                    .capabilities
                    .iter()
                    .any(|capability| !grants.granted_capabilities.contains(capability));
            let now = unix_timestamp();
            let installed_version = InstalledVersionReceipt {
                version: manifest.version.clone(),
                source: verified.source,
                source_id: verified.source_id,
                source_key: verified.source_key,
                publisher: verified.publisher,
                publisher_key: verified.publisher_key,
                publisher_public_key: verified.publisher_public_key,
                package_hash,
                trust: verified.trust,
                channel: verified.channel,
                installed_at_unix_seconds: now,
            };
            let mut receipt = previous_receipt.unwrap_or_else(|| InstallationReceipt {
                schema_version: RECEIPT_SCHEMA_VERSION,
                id: manifest.id.clone(),
                selected_channel: installed_version.channel,
                active: installed_version.clone(),
                previous: None,
                pending: None,
                last_update_failure: None,
                rollback_active: false,
            });
            if is_explicit_source_switch && publisher_changed {
                receipt.previous = None;
                receipt.pending = None;
                receipt.active = installed_version;
                receipt.rollback_active = false;
            } else if receipt.active.version != manifest.version {
                if capability_expansion {
                    receipt.pending = Some(installed_version);
                } else {
                    receipt.previous = Some(receipt.active);
                    receipt.active = installed_version;
                    receipt.pending = None;
                    receipt.rollback_active = false;
                }
            } else {
                receipt.active = installed_version;
            }
            receipt.last_update_failure = None;
            self.save_receipt(&receipt)?;
            if !self.grants_path(&manifest.id).exists() {
                grants.enabled = false;
                self.save_grants(&grants)?;
            }
            Ok(receipt)
        })();
        if stage.exists() {
            let _ = make_tree_writable(&stage);
            let _ = fs::remove_dir_all(&stage);
        }
        install_result
    }

    pub fn installed(&self) -> Result<Vec<InstalledExtension>, CatalogError> {
        let directory = self.paths.receipts_dir();
        let Ok(entries) = fs::read_dir(&directory) else {
            return Ok(Vec::new());
        };
        let mut installed = Vec::new();
        for entry in entries {
            let entry = entry.map_err(|error| io_error(&directory, error))?;
            if entry.path().extension().and_then(|value| value.to_str()) != Some("toml") {
                continue;
            }
            let source =
                fs::read_to_string(entry.path()).map_err(|error| io_error(&entry.path(), error))?;
            let receipt: InstallationReceipt = toml::from_str(&source)
                .map_err(|error| CatalogError::InvalidPackage(error.to_string()))?;
            let package_dir = self.package_dir(&receipt.id, &receipt.active.version);
            let manifest_path = package_dir.join("extension.toml");
            let manifest_source = fs::read_to_string(&manifest_path)
                .map_err(|error| io_error(&manifest_path, error))?;
            let manifest = ExtensionManifest::from_toml(&manifest_source)
                .map_err(|error| CatalogError::InvalidPackage(error.to_string()))?;
            if manifest.id != receipt.id || manifest.version != receipt.active.version {
                return Err(CatalogError::InvalidPackage(format!(
                    "installed manifest does not match receipt for '{}'",
                    receipt.id
                )));
            }
            let grants = self.load_grants(&receipt.id)?;
            installed.push(InstalledExtension {
                receipt,
                manifest,
                grants,
                package_dir,
            });
        }
        installed.sort_by(|left, right| left.receipt.id.cmp(&right.receipt.id));
        Ok(installed)
    }

    fn catalog_releases(
        &self,
        sources: &[RegistrySource],
        diagnostics: &mut Vec<String>,
    ) -> Vec<CatalogExtension> {
        let mut entries = Vec::new();
        for source in sources.iter().filter(|source| source.enabled) {
            match self.load_verified_index(source) {
                Ok(Some(index)) => {
                    entries.extend(index.index.releases.into_iter().map(|release| {
                        let trust = trust_for_release(source, &release);
                        CatalogExtension {
                            release,
                            source: source.clone(),
                            trust,
                            publisher_conflict: false,
                        }
                    }));
                }
                Ok(None) => {}
                Err(error) => diagnostics.push(format!("source '{}': {error}", source.id)),
            }
        }
        let mut grouped: BTreeMap<ExtensionId, Vec<CatalogExtension>> = BTreeMap::new();
        for entry in entries {
            grouped
                .entry(entry.release.id.clone())
                .or_default()
                .push(entry);
        }
        let mut releases = Vec::new();
        for (_, mut candidates) in grouped {
            let sources = candidates
                .iter()
                .map(|candidate| &candidate.source.id)
                .collect::<BTreeSet<_>>();
            let conflict = sources.len() > 1 || has_publisher_conflict(&candidates);
            for candidate in &mut candidates {
                candidate.publisher_conflict = conflict;
            }
            releases.extend(candidates);
        }
        releases
    }

    fn discover(&self, releases: &[CatalogExtension]) -> Vec<CatalogExtension> {
        let mut grouped: BTreeMap<ExtensionId, Vec<CatalogExtension>> = BTreeMap::new();
        for entry in releases.iter().filter(|entry| {
            entry.release.channel == ReleaseChannel::Stable
                && !entry.release.yanked
                && entry.release.min_shilpo_version <= self.shilpo_version
                && entry.release.api_version
                    == Version::parse(SUPPORTED_API_VERSION)
                        .expect("supported API version is valid")
        }) {
            grouped
                .entry(entry.release.id.clone())
                .or_default()
                .push(entry.clone());
        }
        let mut discover = grouped
            .into_values()
            .filter_map(|mut candidates| {
                candidates.sort_by(|left, right| right.release.version.cmp(&left.release.version));
                candidates.into_iter().next()
            })
            .collect::<Vec<_>>();
        discover.sort_by(|left, right| left.release.name.cmp(&right.release.name));
        discover
    }

    fn resolve_updates(
        &self,
        installed: &[InstalledExtension],
        releases: &[CatalogExtension],
    ) -> Vec<ExtensionUpdate> {
        installed
            .iter()
            .map(|extension| {
                let receipt = &extension.receipt;
                let candidates = releases
                    .iter()
                    .filter(|entry| {
                        entry.release.id == receipt.id
                            && entry.release.channel == receipt.selected_channel
                    })
                    .collect::<Vec<_>>();
                let publisher_conflict = candidates
                    .iter()
                    .any(|candidate| candidate.publisher_conflict);
                let installed_yanked = candidates.iter().any(|candidate| {
                    candidate.release.version == receipt.active.version
                        && candidate.release.yanked
                        && source_matches(receipt, candidate)
                        && publisher_matches(receipt, candidate)
                });
                let newer = candidates
                    .iter()
                    .filter(|candidate| candidate.release.version > receipt.active.version)
                    .collect::<Vec<_>>();
                let mut compatible = newer
                    .iter()
                    .filter(|candidate| {
                        !candidate.release.yanked
                            && candidate.release.min_shilpo_version <= self.shilpo_version
                            && candidate.release.api_version
                                == Version::parse(SUPPORTED_API_VERSION)
                                    .expect("supported API version is valid")
                            && source_matches(receipt, candidate)
                            && publisher_matches(receipt, candidate)
                    })
                    .map(|candidate| (**candidate).clone())
                    .collect::<Vec<_>>();
                compatible.sort_by(|left, right| right.release.version.cmp(&left.release.version));
                let candidate = compatible.into_iter().next();
                let state = if receipt.pending.is_some() {
                    UpdateState::AwaitingPermissionReview
                } else if receipt.last_update_failure.is_some() {
                    UpdateState::FailedUsingPreviousVersion
                } else if receipt.rollback_active {
                    UpdateState::RollbackActive
                } else if publisher_conflict {
                    UpdateState::PublisherConflict
                } else if installed_yanked {
                    UpdateState::Yanked
                } else if candidate.is_some() {
                    UpdateState::Available
                } else if newer.iter().any(|candidate| {
                    !source_matches(receipt, candidate) || !publisher_matches(receipt, candidate)
                }) {
                    UpdateState::PublisherConflict
                } else if !newer.is_empty() {
                    UpdateState::Incompatible
                } else {
                    UpdateState::UpToDate
                };
                ExtensionUpdate {
                    id: receipt.id.clone(),
                    installed_version: receipt.active.version.clone(),
                    available: candidate,
                    state,
                }
            })
            .collect()
    }

    fn sources(&self) -> Result<Vec<RegistrySource>, CatalogError> {
        let path = self.paths.sources_path();
        let stored = if path.exists() {
            let source = fs::read_to_string(&path).map_err(|error| io_error(&path, error))?;
            toml::from_str::<StoredSources>(&source)
                .map_err(|error| CatalogError::InvalidRegistry(error.to_string()))?
        } else {
            StoredSources::default()
        };
        for registry in &stored.source {
            if registry.official {
                return Err(CatalogError::InvalidRegistry(
                    "configured sources cannot claim official trust".into(),
                ));
            }
            validate_source(registry)?;
        }
        let mut sources = official_sources();
        sources.extend(stored.source);
        Ok(sources)
    }

    fn save_sources(&self, sources: &[RegistrySource]) -> Result<(), CatalogError> {
        let source = toml::to_string_pretty(&StoredSources {
            source: sources
                .iter()
                .filter(|source| !source.official)
                .cloned()
                .collect(),
        })
        .map_err(|error| CatalogError::InvalidRegistry(error.to_string()))?;
        write_atomic(&self.paths.sources_path(), source.as_bytes())
    }

    fn save_grants(&self, grants: &StoredGrants) -> Result<(), CatalogError> {
        let source = toml::to_string_pretty(grants)
            .map_err(|error| CatalogError::InvalidPackage(error.to_string()))?;
        write_atomic(&self.grants_path(&grants.extension_id), source.as_bytes())
    }

    fn save_receipt(&self, receipt: &InstallationReceipt) -> Result<(), CatalogError> {
        let source = toml::to_string_pretty(receipt)
            .map_err(|error| CatalogError::InvalidPackage(error.to_string()))?;
        write_atomic(&self.receipt_path(&receipt.id), source.as_bytes())
    }

    fn record_update_failure(
        &self,
        extension_id: &ExtensionId,
        error: &CatalogError,
    ) -> Result<(), CatalogError> {
        let mut receipt = self.receipt(extension_id)?;
        receipt.last_update_failure = Some(error.to_string());
        self.save_receipt(&receipt)
    }

    fn store_verified_index(
        &self,
        source: &RegistrySource,
        bytes: &[u8],
    ) -> Result<(), CatalogError> {
        if bytes.len() > MAX_INDEX_BYTES {
            return Err(CatalogError::InvalidRegistry(
                "registry index exceeds 8 MiB".into(),
            ));
        }
        let index: SignedRegistryIndex = serde_json::from_slice(bytes)
            .map_err(|error| CatalogError::InvalidRegistry(error.to_string()))?;
        verify_registry_index(source, &index)?;
        write_atomic(&self.index_path(&source.id), bytes)
    }

    fn load_verified_index(
        &self,
        source: &RegistrySource,
    ) -> Result<Option<SignedRegistryIndex>, CatalogError> {
        let path = self.index_path(&source.id);
        if !path.exists() {
            return Ok(None);
        }
        let bytes = fs::read(&path).map_err(|error| io_error(&path, error))?;
        let index: SignedRegistryIndex = serde_json::from_slice(&bytes)
            .map_err(|error| CatalogError::InvalidRegistry(error.to_string()))?;
        verify_registry_index(source, &index)?;
        Ok(Some(index))
    }

    fn extension_dir(&self, extension_id: &ExtensionId) -> PathBuf {
        self.paths.installed_dir().join(extension_id.as_str())
    }

    fn package_dir(&self, extension_id: &ExtensionId, version: &Version) -> PathBuf {
        self.extension_dir(extension_id).join(version.to_string())
    }

    fn receipt_path(&self, extension_id: &ExtensionId) -> PathBuf {
        self.paths
            .receipts_dir()
            .join(format!("{extension_id}.toml"))
    }

    fn grants_path(&self, extension_id: &ExtensionId) -> PathBuf {
        self.paths.grants_dir().join(format!("{extension_id}.toml"))
    }

    fn index_path(&self, source_id: &str) -> PathBuf {
        self.paths.indexes_dir().join(format!("{source_id}.json"))
    }
}

fn official_sources() -> Vec<RegistrySource> {
    option_env!("SHILPO_OFFICIAL_EXTENSIONS_ROOT_KEY").map_or_else(Vec::new, |root_public_key| {
        vec![RegistrySource {
            id: OFFICIAL_SOURCE_ID.into(),
            name: OFFICIAL_SOURCE_NAME.into(),
            index_url: option_env!("SHILPO_OFFICIAL_EXTENSIONS_INDEX_URL")
                .unwrap_or(OFFICIAL_SOURCE_URL)
                .into(),
            root_public_key: root_public_key.into(),
            official: true,
            enabled: true,
        }]
    })
}

enum PackageProvenance {
    Local(Option<PackageSignature>),
    Direct(String, Option<PackageSignature>),
    Registry(Box<CatalogExtension>),
}

struct VerifiedProvenance {
    source: String,
    source_id: Option<String>,
    source_key: Option<String>,
    publisher: Option<String>,
    publisher_key: Option<String>,
    publisher_public_key: Option<String>,
    trust: TrustState,
    channel: ReleaseChannel,
    key_rotation: Option<KeyRotationDelegation>,
    expected_release: Option<RegistryRelease>,
}

impl PackageProvenance {
    fn verify(self, package_hash: &str) -> Result<VerifiedProvenance, CatalogError> {
        match self {
            Self::Local(signature) => {
                verify_direct_provenance("local-archive".into(), signature, package_hash)
            }
            Self::Direct(url, signature) => {
                verify_direct_provenance(format!("url:{url}"), signature, package_hash)
            }
            Self::Registry(candidate) => {
                let candidate = *candidate;
                let release = candidate.release;
                if release.package_hash != package_hash {
                    return Err(CatalogError::InvalidSignature(
                        "downloaded package hash does not match the signed release".into(),
                    ));
                }
                verify_release_signature(&release)?;
                let fingerprint = public_key_fingerprint(&release.publisher_public_key)?;
                Ok(VerifiedProvenance {
                    source: format!("registry:{}", candidate.source.id),
                    source_id: Some(candidate.source.id.clone()),
                    source_key: Some(candidate.source.root_public_key.clone()),
                    publisher: Some(release.publisher.clone()),
                    publisher_key: Some(fingerprint),
                    publisher_public_key: Some(release.publisher_public_key.clone()),
                    trust: candidate.trust,
                    channel: release.channel,
                    key_rotation: release.key_rotation.clone(),
                    expected_release: Some(release),
                })
            }
        }
    }
}

fn verify_direct_provenance(
    source: String,
    signature: Option<PackageSignature>,
    package_hash: &str,
) -> Result<VerifiedProvenance, CatalogError> {
    let Some(package_signature) = signature else {
        return Ok(VerifiedProvenance {
            source,
            source_id: None,
            source_key: None,
            publisher: None,
            publisher_key: None,
            publisher_public_key: None,
            trust: TrustState::Unverified,
            channel: ReleaseChannel::Stable,
            key_rotation: None,
            expected_release: None,
        });
    };
    if package_signature.schema_version != 1 || package_signature.package_hash != package_hash {
        return Err(CatalogError::InvalidSignature(
            "package signature hash or schema does not match".into(),
        ));
    }
    verify_signature(
        &package_signature.public_key,
        package_signing_message(
            &package_signature.publisher,
            &package_signature.package_hash,
        )
        .as_bytes(),
        &package_signature.signature,
    )?;
    let fingerprint = public_key_fingerprint(&package_signature.public_key)?;
    Ok(VerifiedProvenance {
        source,
        source_id: None,
        source_key: None,
        publisher: Some(package_signature.publisher),
        publisher_key: Some(fingerprint),
        publisher_public_key: Some(package_signature.public_key),
        trust: TrustState::SignedThirdParty,
        channel: ReleaseChannel::Stable,
        key_rotation: package_signature.key_rotation,
        expected_release: None,
    })
}

fn verify_provenance_continuity(
    receipt: Option<&InstallationReceipt>,
    verified: &VerifiedProvenance,
) -> Result<(), CatalogError> {
    let Some(receipt) = receipt else {
        return Ok(());
    };
    if let Some(installed_source_id) = pinned_source_id(receipt) {
        let incoming_source_id = verified.source_id.as_deref();
        if incoming_source_id != Some(installed_source_id) {
            return Err(CatalogError::PublisherConflict(format!(
                "'{}' is pinned to source '{}', cannot install from '{}'",
                receipt.id,
                installed_source_id,
                incoming_source_id.unwrap_or("non-registry")
            )));
        }
        if let Some(installed_source_key) = &receipt.active.source_key
            && verified.source_key.as_deref() != Some(installed_source_key.as_str())
        {
            return Err(CatalogError::PublisherConflict(format!(
                "'{}' source root public key has changed",
                receipt.id
            )));
        }
    } else if verified.source_id.is_some() {
        return Err(CatalogError::PublisherConflict(format!(
            "'{}' was installed locally/directly, cannot update from registry without explicit source switch",
            receipt.id
        )));
    }
    verify_publisher_continuity(Some(receipt), verified)
}

fn pinned_source_id(receipt: &InstallationReceipt) -> Option<&str> {
    if let Some(id) = &receipt.active.source_id {
        Some(id.as_str())
    } else {
        receipt.active.source.strip_prefix("registry:")
    }
}

fn source_matches(receipt: &InstallationReceipt, candidate: &CatalogExtension) -> bool {
    let Some(source_id) = pinned_source_id(receipt) else {
        return false;
    };
    if source_id != candidate.source.id {
        return false;
    }
    if let Some(source_key) = &receipt.active.source_key
        && source_key != &candidate.source.root_public_key
    {
        return false;
    }
    true
}

fn verify_publisher_continuity(
    receipt: Option<&InstallationReceipt>,
    verified: &VerifiedProvenance,
) -> Result<(), CatalogError> {
    let Some(receipt) = receipt else {
        return Ok(());
    };
    if receipt.active.publisher_key == verified.publisher_key {
        return Ok(());
    }
    let Some(rotation) = &verified.key_rotation else {
        return Err(CatalogError::PublisherConflict(format!(
            "'{}' changed publisher identity without a delegation",
            receipt.id
        )));
    };
    let old_key = receipt
        .active
        .publisher_public_key
        .as_deref()
        .ok_or_else(|| {
            CatalogError::PublisherConflict("the previous publisher was unsigned".into())
        })?;
    if rotation.previous_public_key != old_key
        || Some(public_key_fingerprint(&rotation.next_public_key)?) != verified.publisher_key
        || verified.publisher_public_key.as_deref() != Some(rotation.next_public_key.as_str())
    {
        return Err(CatalogError::PublisherConflict(
            "key-rotation identity does not match the installation receipt".into(),
        ));
    }
    if rotation.authorized_by_registry && verified.source.starts_with("registry:") {
        return Ok(());
    }
    verify_signature(
        &rotation.previous_public_key,
        rotation_message(&rotation.next_public_key).as_bytes(),
        &rotation.signature,
    )
    .map_err(|error| CatalogError::PublisherConflict(error.to_string()))
}

fn publisher_matches(receipt: &InstallationReceipt, candidate: &CatalogExtension) -> bool {
    public_key_fingerprint(&candidate.release.publisher_public_key)
        .ok()
        .is_some_and(|fingerprint| {
            receipt.active.publisher_key.as_deref() == Some(fingerprint.as_str())
                || candidate
                    .release
                    .key_rotation
                    .as_ref()
                    .is_some_and(|rotation| {
                        public_key_fingerprint(&rotation.previous_public_key)
                            .ok()
                            .as_deref()
                            == receipt.active.publisher_key.as_deref()
                    })
        })
}

fn has_publisher_conflict(candidates: &[CatalogExtension]) -> bool {
    let keys = candidates
        .iter()
        .map(|entry| public_key_fingerprint(&entry.release.publisher_public_key))
        .collect::<Result<BTreeSet<_>, _>>();
    let Ok(keys) = keys else {
        return true;
    };
    let Some(first) = keys.iter().next().cloned() else {
        return false;
    };
    let mut connected = BTreeSet::from([first]);
    loop {
        let before = connected.len();
        for candidate in candidates {
            let Ok(current) = public_key_fingerprint(&candidate.release.publisher_public_key)
            else {
                return true;
            };
            let Some(rotation) = &candidate.release.key_rotation else {
                continue;
            };
            let Ok(previous) = public_key_fingerprint(&rotation.previous_public_key) else {
                return true;
            };
            if connected.contains(&current) || connected.contains(&previous) {
                connected.insert(current);
                connected.insert(previous);
            }
        }
        if connected.len() == before {
            break;
        }
    }
    !keys.is_subset(&connected)
}

fn trust_for_release(source: &RegistrySource, release: &RegistryRelease) -> TrustState {
    if source.official {
        TrustState::Official
    } else if release.verified_publisher {
        TrustState::VerifiedPublisher
    } else {
        TrustState::SignedThirdParty
    }
}

fn verify_manifest_against_release(
    manifest: &ExtensionManifest,
    release: &RegistryRelease,
) -> Result<(), CatalogError> {
    if manifest.id != release.id
        || manifest.version != release.version
        || manifest.api_version != release.api_version
        || manifest.min_shilpo_version != release.min_shilpo_version
        || capabilities_hash(&manifest.capabilities)? != release.capabilities_hash
    {
        return Err(CatalogError::InvalidPackage(
            "manifest does not match signed release metadata".into(),
        ));
    }
    Ok(())
}

fn extract_package(package: &Path, destination: &Path) -> Result<(), CatalogError> {
    let file = File::open(package).map_err(|error| io_error(package, error))?;
    let decoder = GzDecoder::new(file);
    let mut archive = Archive::new(decoder);
    let entries = archive
        .entries()
        .map_err(|error| CatalogError::InvalidPackage(error.to_string()))?;
    let mut total = 0u64;
    for entry in entries {
        let mut entry = entry.map_err(|error| CatalogError::InvalidPackage(error.to_string()))?;
        let relative = entry
            .path()
            .map_err(|error| CatalogError::InvalidPackage(error.to_string()))?
            .into_owned();
        if relative.as_os_str().is_empty()
            || relative.is_absolute()
            || relative
                .components()
                .any(|component| !matches!(component, Component::Normal(_)))
        {
            return Err(CatalogError::InvalidPackage(format!(
                "archive path '{}' escapes the package",
                relative.display()
            )));
        }
        let kind = entry.header().entry_type();
        let target = destination.join(&relative);
        if kind.is_dir() {
            fs::create_dir_all(&target).map_err(|error| io_error(&target, error))?;
            continue;
        }
        if !kind.is_file() {
            return Err(CatalogError::InvalidPackage(format!(
                "archive entry '{}' is not a regular file",
                relative.display()
            )));
        }
        total = total.saturating_add(entry.size());
        if total > MAX_PACKAGE_BYTES {
            return Err(CatalogError::InvalidPackage(
                "expanded package exceeds 64 MiB".into(),
            ));
        }
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).map_err(|error| io_error(parent, error))?;
        }
        let mut output = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&target)
            .map_err(|error| io_error(&target, error))?;
        std::io::copy(&mut entry, &mut output).map_err(|error| io_error(&target, error))?;
        output
            .sync_all()
            .map_err(|error| io_error(&target, error))?;
    }
    Ok(())
}

fn mark_files_read_only(path: &Path) -> Result<(), CatalogError> {
    for entry in fs::read_dir(path).map_err(|error| io_error(path, error))? {
        let entry = entry.map_err(|error| io_error(path, error))?;
        let metadata = entry
            .metadata()
            .map_err(|error| io_error(&entry.path(), error))?;
        if metadata.is_dir() {
            mark_files_read_only(&entry.path())?;
        } else if metadata.is_file() {
            let mut permissions = metadata.permissions();
            permissions.set_readonly(true);
            fs::set_permissions(entry.path(), permissions)
                .map_err(|error| io_error(&entry.path(), error))?;
        }
    }
    Ok(())
}

fn make_tree_writable(path: &Path) -> Result<(), CatalogError> {
    for entry in fs::read_dir(path).map_err(|error| io_error(path, error))? {
        let entry = entry.map_err(|error| io_error(path, error))?;
        let metadata = entry
            .metadata()
            .map_err(|error| io_error(&entry.path(), error))?;
        if metadata.is_dir() {
            make_tree_writable(&entry.path())?;
        } else {
            let mut permissions = metadata.permissions();
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt as _;
                permissions.set_mode(permissions.mode() | 0o200);
            }
            #[cfg(not(unix))]
            permissions.set_readonly(false);
            fs::set_permissions(entry.path(), permissions)
                .map_err(|error| io_error(&entry.path(), error))?;
        }
    }
    Ok(())
}

fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), CatalogError> {
    let parent = path
        .parent()
        .ok_or_else(|| CatalogError::Io(format!("{} has no parent", path.display())))?;
    fs::create_dir_all(parent).map_err(|error| io_error(parent, error))?;
    let temporary = parent.join(format!(".write-{}.tmp", unique_suffix()));
    let result = (|| {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
            .map_err(|error| io_error(&temporary, error))?;
        file.write_all(bytes)
            .map_err(|error| io_error(&temporary, error))?;
        file.sync_all()
            .map_err(|error| io_error(&temporary, error))?;
        fs::rename(&temporary, path).map_err(|error| io_error(path, error))?;
        if let Ok(directory) = File::open(parent) {
            let _ = directory.sync_all();
        }
        Ok(())
    })();
    if temporary.exists() {
        let _ = fs::remove_file(temporary);
    }
    result
}

fn fetch_to_staging(url: &str, staging: &Path, max_bytes: u64) -> Result<PathBuf, CatalogError> {
    let bytes = fetch_bytes(url, max_bytes)?;
    fs::create_dir_all(staging).map_err(|error| io_error(staging, error))?;
    let path = staging.join(format!("download-{}.shilpo-ext", unique_suffix()));
    write_atomic(&path, &bytes)?;
    Ok(path)
}

fn fetch_bytes(url: &str, max_bytes: u64) -> Result<Vec<u8>, CatalogError> {
    if let Some(path) = url.strip_prefix("file://") {
        return read_limited(Path::new(path), max_bytes);
    }
    if !url.contains("://") {
        return read_limited(Path::new(url), max_bytes);
    }
    if !url.starts_with("https://") {
        return Err(CatalogError::InvalidRegistry(
            "remote extension sources must use HTTPS".into(),
        ));
    }
    let url = url.to_owned();
    std::thread::spawn(move || {
        let response = reqwest::blocking::get(&url)
            .map_err(|error| CatalogError::Io(format!("failed to fetch {url}: {error}")))?;
        if !response.status().is_success() {
            return Err(CatalogError::Io(format!(
                "{url} returned HTTP {}",
                response.status()
            )));
        }
        if response
            .content_length()
            .is_some_and(|length| length > max_bytes)
        {
            return Err(CatalogError::Io(format!(
                "{url} exceeds the download limit"
            )));
        }
        let bytes = response
            .bytes()
            .map_err(|error| CatalogError::Io(format!("failed to read {url}: {error}")))?;
        if bytes.len() as u64 > max_bytes {
            return Err(CatalogError::Io(format!(
                "{url} exceeds the download limit"
            )));
        }
        Ok(bytes.to_vec())
    })
    .join()
    .map_err(|_| CatalogError::Io("download worker panicked".into()))?
}

fn read_limited(path: &Path, max_bytes: u64) -> Result<Vec<u8>, CatalogError> {
    let metadata = fs::metadata(path).map_err(|error| io_error(path, error))?;
    if metadata.len() > max_bytes {
        return Err(CatalogError::Io(format!(
            "{} exceeds the read limit",
            path.display()
        )));
    }
    fs::read(path).map_err(|error| io_error(path, error))
}

fn io_error(path: &Path, error: std::io::Error) -> CatalogError {
    CatalogError::Io(format!("{}: {error}", path.display()))
}

fn unix_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn unique_suffix() -> String {
    format!(
        "{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    )
}

#[cfg(test)]
mod tests {
    use std::fs;

    use base64::Engine as _;
    use base64::engine::general_purpose::STANDARD as BASE64;
    use flate2::Compression;
    use flate2::write::GzEncoder;
    use shilpo_ext_api::SecretPurpose;
    use tar::{Builder, Header};

    use super::*;
    use crate::secrets::SecretBroker;

    fn test_root(name: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!("shilpo-catalog-{name}-{}", unique_suffix()));
        fs::create_dir_all(&root).unwrap();
        root
    }

    fn catalog(root: &Path) -> ExtensionCatalog {
        ExtensionCatalog::open(
            CatalogPaths::new(root.join("data"), root.join("config")),
            Version::parse(CURRENT_SHILPO_VERSION).unwrap(),
        )
    }

    fn package(root: &Path, version: &str, capabilities: &str) -> PathBuf {
        let source = root.join(format!("source-{version}"));
        let output = root.join(format!("dist-{version}"));
        fs::create_dir_all(&source).unwrap();
        fs::write(
            source.join("extension.toml"),
            format!(
                r#"
id = "io.github.shilpo.catalog-test"
name = "Catalog Test"
version = "{version}"
{capabilities}
"#
            ),
        )
        .unwrap();
        ExtensionCli::pack(&source, &output).artifact.unwrap()
    }

    #[test]
    fn local_install_is_disabled_and_grants_fail_closed() {
        let root = test_root("grants");
        let catalog = catalog(&root);
        let package = package(&root, "1.0.0", "");
        let receipt = catalog.install_local(&package).unwrap();
        assert_eq!(receipt.active.trust, TrustState::Unverified);
        assert!(!catalog.load_grants(&receipt.id).unwrap().enabled);
        assert!(catalog.active_packages().unwrap().is_empty());

        catalog.set_enabled(&receipt.id, true).unwrap();
        assert_eq!(catalog.active_packages().unwrap().len(), 1);
        fs::write(catalog.grants_path(&receipt.id), "not valid toml").unwrap();
        assert!(catalog.load_grants(&receipt.id).is_err());
        assert!(catalog.active_packages().is_err());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn active_install_capabilities_can_be_reviewed_without_a_pending_update() {
        let root = test_root("active-grants");
        let catalog = catalog(&root);
        let package = package(
            &root,
            "1.0.0",
            r#"
[[capabilities]]
kind = "network:http"
hosts = ["api.open-meteo.com"]
paths = ["/v1/forecast*"]
"#,
        );
        let receipt = catalog.install_local(&package).unwrap();
        let requested = catalog.requested_capabilities(&receipt.id).unwrap();
        assert_eq!(requested.len(), 1);
        let grants = catalog
            .approve_capabilities(&receipt.id, requested.clone())
            .unwrap();
        assert_eq!(grants.granted_capabilities, requested);
        assert!(
            catalog
                .approve_capabilities(&receipt.id, vec![Capability::ClipboardRead])
                .is_err()
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn updates_stage_capability_expansion_and_support_rollback() {
        let root = test_root("update");
        let catalog = catalog(&root);
        let first = catalog.install_local(&package(&root, "1.0.0", "")).unwrap();
        let second = catalog.install_local(&package(&root, "2.0.0", "")).unwrap();
        assert_eq!(second.active.version, Version::parse("2.0.0").unwrap());
        assert_eq!(
            second
                .previous
                .as_ref()
                .map(|version| version.version.clone()),
            Some(Version::parse("1.0.0").unwrap())
        );

        let rolled_back = catalog.rollback(&first.id).unwrap();
        assert_eq!(rolled_back.active.version, Version::parse("1.0.0").unwrap());

        let pending = catalog
            .install_local(&package(
                &root,
                "3.0.0",
                r#"
[[capabilities]]
kind = "notifications:show"
"#,
            ))
            .unwrap();
        assert_eq!(
            pending
                .pending
                .as_ref()
                .map(|version| version.version.clone()),
            Some(Version::parse("3.0.0").unwrap())
        );
        assert_eq!(pending.active.version, Version::parse("1.0.0").unwrap());
        let requested = catalog.pending_capabilities(&first.id).unwrap();
        let activated = catalog.approve_pending(&first.id, requested).unwrap();
        assert_eq!(activated.active.version, Version::parse("3.0.0").unwrap());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn signed_packages_enforce_integrity_and_publisher_continuity() {
        let root = test_root("signature");
        let catalog = catalog(&root);
        let (first_private, first_public) = generate_signing_key().unwrap();
        let (second_private, second_public) = generate_signing_key().unwrap();
        let first_package = package(&root, "1.0.0", "");
        sign_package(&first_package, "Alice", &first_private).unwrap();
        let first = catalog.install_local(&first_package).unwrap();
        assert_eq!(first.active.trust, TrustState::SignedThirdParty);

        let second_package = package(&root, "2.0.0", "");
        let second_sidecar = sign_package(&second_package, "Alice", &second_private).unwrap();
        assert!(matches!(
            catalog.install_local(&second_package),
            Err(CatalogError::PublisherConflict(_))
        ));

        let old_pair = decode_key_pair(&first_private).unwrap();
        let mut signature: PackageSignature =
            serde_json::from_slice(&fs::read(&second_sidecar).unwrap()).unwrap();
        signature.key_rotation = Some(KeyRotationDelegation {
            previous_public_key: first_public,
            next_public_key: second_public.clone(),
            signature: BASE64.encode(old_pair.sign(rotation_message(&second_public).as_bytes())),
            authorized_by_registry: false,
        });
        fs::write(
            &second_sidecar,
            serde_json::to_vec_pretty(&signature).unwrap(),
        )
        .unwrap();
        assert_eq!(
            catalog
                .install_local(&second_package)
                .unwrap()
                .active
                .version,
            Version::parse("2.0.0").unwrap()
        );

        let tampered = package(&root, "3.0.0", "");
        sign_package(&tampered, "Alice", &second_private).unwrap();
        OpenOptions::new()
            .append(true)
            .open(&tampered)
            .unwrap()
            .write_all(b"tampered")
            .unwrap();
        assert!(matches!(
            catalog.install_local(&tampered),
            Err(CatalogError::InvalidSignature(_))
        ));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn signed_registry_drives_discovery_and_updates() {
        let root = test_root("registry");
        let catalog = catalog(&root);
        let (publisher_private, publisher_public) = generate_signing_key().unwrap();
        let (registry_private, registry_public) = generate_signing_key().unwrap();
        let first_package = package(&root, "1.0.0", "");
        sign_package(&first_package, "Alice", &publisher_private).unwrap();
        let second_package = package(&root, "2.0.0", "");
        let mut first_release = RegistryRelease {
            id: ExtensionId::new("io.github.shilpo.catalog-test").unwrap(),
            name: "Catalog Test".into(),
            description: Some("A registry fixture".into()),
            publisher: "Alice".into(),
            version: Version::parse("1.0.0").unwrap(),
            api_version: Version::parse(SUPPORTED_API_VERSION).unwrap(),
            min_shilpo_version: Version::parse(CURRENT_SHILPO_VERSION).unwrap(),
            channel: ReleaseChannel::Stable,
            package_url: first_package.display().to_string(),
            package_hash: hash_file(&first_package).unwrap(),
            publisher_public_key: publisher_public.clone(),
            publisher_signature: String::new(),
            capabilities_hash: String::new(),
            capabilities: Vec::new(),
            published_at: "2026-07-26T11:00:00Z".into(),
            yanked: false,
            verified_publisher: true,
            open_source: true,
            data_only: true,
            key_rotation: None,
        };
        sign_release(&mut first_release, &publisher_private).unwrap();
        let mut release = RegistryRelease {
            id: ExtensionId::new("io.github.shilpo.catalog-test").unwrap(),
            name: "Catalog Test".into(),
            description: Some("A registry fixture".into()),
            publisher: "Alice".into(),
            version: Version::parse("2.0.0").unwrap(),
            api_version: Version::parse(SUPPORTED_API_VERSION).unwrap(),
            min_shilpo_version: Version::parse(CURRENT_SHILPO_VERSION).unwrap(),
            channel: ReleaseChannel::Stable,
            package_url: second_package.display().to_string(),
            package_hash: hash_file(&second_package).unwrap(),
            publisher_public_key: publisher_public,
            publisher_signature: String::new(),
            capabilities_hash: String::new(),
            capabilities: Vec::new(),
            published_at: "2026-07-26T12:00:00Z".into(),
            yanked: false,
            verified_publisher: true,
            open_source: true,
            data_only: true,
            key_rotation: None,
        };
        sign_release(&mut release, &publisher_private).unwrap();
        let initial_signed = sign_registry_index(
            RegistryIndex {
                schema_version: REGISTRY_SCHEMA_VERSION,
                source_id: "community".into(),
                generated_at: "2026-07-26T11:00:00Z".into(),
                releases: vec![first_release],
            },
            &registry_private,
        )
        .unwrap();
        catalog
            .add_source(RegistrySource {
                id: "community".into(),
                name: "Community".into(),
                index_url: root.join("index.json").display().to_string(),
                root_public_key: registry_public,
                official: false,
                enabled: true,
            })
            .unwrap();
        catalog
            .store_index_bytes("community", &serde_json::to_vec(&initial_signed).unwrap())
            .unwrap();
        let installed = catalog
            .install_from_catalog(&ExtensionId::new("io.github.shilpo.catalog-test").unwrap())
            .unwrap();
        assert_eq!(installed.active.version, Version::parse("1.0.0").unwrap());
        assert_eq!(installed.active.source_id.as_deref(), Some("community"));

        let signed = sign_registry_index(
            RegistryIndex {
                schema_version: REGISTRY_SCHEMA_VERSION,
                source_id: "community".into(),
                generated_at: "2026-07-26T12:00:00Z".into(),
                releases: vec![signed_release_clone(&release, &publisher_private)],
            },
            &registry_private,
        )
        .unwrap();
        let bytes = serde_json::to_vec_pretty(&signed).unwrap();
        catalog.store_index_bytes("community", &bytes).unwrap();
        let snapshot = catalog.snapshot();
        assert_eq!(snapshot.discover.len(), 1);
        assert_eq!(snapshot.discover[0].trust, TrustState::VerifiedPublisher);
        assert_eq!(snapshot.updates[0].state, UpdateState::Available);
        assert_eq!(
            catalog
                .install_from_catalog(&installed.id)
                .unwrap()
                .active
                .version,
            Version::parse("2.0.0").unwrap()
        );

        let mut yanked = signed.index.releases[0].clone();
        yanked.yanked = true;
        sign_release(&mut yanked, &publisher_private).unwrap();
        let yanked_index = sign_registry_index(
            RegistryIndex {
                schema_version: REGISTRY_SCHEMA_VERSION,
                source_id: "community".into(),
                generated_at: "2026-07-26T13:00:00Z".into(),
                releases: vec![yanked],
            },
            &registry_private,
        )
        .unwrap();
        catalog
            .store_index_bytes("community", &serde_json::to_vec(&yanked_index).unwrap())
            .unwrap();
        assert_eq!(catalog.snapshot().updates[0].state, UpdateState::Yanked);

        let mut incompatible = signed.index.releases[0].clone();
        incompatible.version = Version::parse("3.0.0").unwrap();
        incompatible.min_shilpo_version = Version::parse("99.0.0").unwrap();
        sign_release(&mut incompatible, &publisher_private).unwrap();
        let incompatible_index = sign_registry_index(
            RegistryIndex {
                schema_version: REGISTRY_SCHEMA_VERSION,
                source_id: "community".into(),
                generated_at: "2026-07-26T14:00:00Z".into(),
                releases: vec![signed.index.releases[0].clone(), incompatible],
            },
            &registry_private,
        )
        .unwrap();
        catalog
            .store_index_bytes(
                "community",
                &serde_json::to_vec(&incompatible_index).unwrap(),
            )
            .unwrap();
        assert_eq!(
            catalog.snapshot().updates[0].state,
            UpdateState::Incompatible
        );

        let mut beta = signed.index.releases[0].clone();
        beta.version = Version::parse("3.0.0-beta.1").unwrap();
        beta.channel = ReleaseChannel::Beta;
        sign_release(&mut beta, &publisher_private).unwrap();
        let channel_index = sign_registry_index(
            RegistryIndex {
                schema_version: REGISTRY_SCHEMA_VERSION,
                source_id: "community".into(),
                generated_at: "2026-07-26T15:00:00Z".into(),
                releases: vec![signed.index.releases[0].clone(), beta],
            },
            &registry_private,
        )
        .unwrap();
        catalog
            .store_index_bytes("community", &serde_json::to_vec(&channel_index).unwrap())
            .unwrap();
        catalog
            .set_channel(&installed.id, ReleaseChannel::Beta)
            .unwrap();
        let beta_update = catalog.snapshot();
        assert_eq!(beta_update.updates[0].state, UpdateState::Available);
        assert_eq!(
            beta_update.updates[0]
                .available
                .as_ref()
                .unwrap()
                .release
                .channel,
            ReleaseChannel::Beta
        );
        catalog
            .set_channel(&installed.id, ReleaseChannel::Stable)
            .unwrap();
        catalog.store_index_bytes("community", &bytes).unwrap();

        let mut tampered: SignedRegistryIndex = serde_json::from_slice(&bytes).unwrap();
        tampered.index.generated_at = "tampered".into();
        assert!(
            catalog
                .store_index_bytes("community", &serde_json::to_vec(&tampered).unwrap())
                .is_err()
        );

        let (other_publisher_private, _) = generate_signing_key().unwrap();
        let (other_registry_private, other_registry_public) = generate_signing_key().unwrap();
        let third_package = package(&root, "3.0.0", "");
        let mut conflicting_release = signed.index.releases[0].clone();
        conflicting_release.version = Version::parse("3.0.0").unwrap();
        conflicting_release.package_url = third_package.display().to_string();
        conflicting_release.package_hash = hash_file(&third_package).unwrap();
        sign_release(&mut conflicting_release, &other_publisher_private).unwrap();
        let conflicting_index = sign_registry_index(
            RegistryIndex {
                schema_version: REGISTRY_SCHEMA_VERSION,
                source_id: "other".into(),
                generated_at: "2026-07-27T12:00:00Z".into(),
                releases: vec![conflicting_release],
            },
            &other_registry_private,
        )
        .unwrap();
        catalog
            .add_source(RegistrySource {
                id: "other".into(),
                name: "Other".into(),
                index_url: root.join("other.json").display().to_string(),
                root_public_key: other_registry_public,
                official: false,
                enabled: true,
            })
            .unwrap();
        catalog
            .store_index_bytes("other", &serde_json::to_vec(&conflicting_index).unwrap())
            .unwrap();
        let collision = catalog.snapshot();
        assert!(collision.discover[0].publisher_conflict);
        assert_eq!(collision.updates[0].state, UpdateState::PublisherConflict);
        fs::remove_dir_all(root).unwrap();
    }

    fn signed_release_clone(release: &RegistryRelease, key: &str) -> RegistryRelease {
        let mut clone = release.clone();
        sign_release(&mut clone, key).unwrap();
        clone
    }

    #[test]
    fn install_receipts_pin_source_id_source_key_and_package_signer() {
        let root = test_root("receipt-pinning");
        let catalog = catalog(&root);
        let (publisher_private, publisher_public) = generate_signing_key().unwrap();
        let (registry_private, registry_public) = generate_signing_key().unwrap();
        let first_package = package(&root, "1.0.0", "");
        sign_package(&first_package, "Alice", &publisher_private).unwrap();

        let mut release = RegistryRelease {
            id: ExtensionId::new("io.github.shilpo.catalog-test").unwrap(),
            name: "Catalog Test".into(),
            description: Some("A registry fixture".into()),
            publisher: "Alice".into(),
            version: Version::parse("1.0.0").unwrap(),
            api_version: Version::parse(SUPPORTED_API_VERSION).unwrap(),
            min_shilpo_version: Version::parse(CURRENT_SHILPO_VERSION).unwrap(),
            channel: ReleaseChannel::Stable,
            package_url: first_package.display().to_string(),
            package_hash: hash_file(&first_package).unwrap(),
            publisher_public_key: publisher_public.clone(),
            publisher_signature: String::new(),
            capabilities_hash: String::new(),
            capabilities: Vec::new(),
            published_at: "2026-07-26T12:00:00Z".into(),
            yanked: false,
            verified_publisher: true,
            open_source: true,
            data_only: true,
            key_rotation: None,
        };
        sign_release(&mut release, &publisher_private).unwrap();
        let signed = sign_registry_index(
            RegistryIndex {
                schema_version: REGISTRY_SCHEMA_VERSION,
                source_id: "community".into(),
                generated_at: "2026-07-26T12:00:00Z".into(),
                releases: vec![release],
            },
            &registry_private,
        )
        .unwrap();
        catalog
            .add_source(RegistrySource {
                id: "community".into(),
                name: "Community".into(),
                index_url: root.join("index.json").display().to_string(),
                root_public_key: registry_public.clone(),
                official: false,
                enabled: true,
            })
            .unwrap();
        let bytes = serde_json::to_vec_pretty(&signed).unwrap();
        catalog.store_index_bytes("community", &bytes).unwrap();

        let receipt = catalog
            .install_from_catalog(&ExtensionId::new("io.github.shilpo.catalog-test").unwrap())
            .unwrap();
        assert_eq!(receipt.active.source, "registry:community");
        assert_eq!(receipt.active.source_id.as_deref(), Some("community"));
        assert_eq!(
            receipt.active.source_key.as_deref(),
            Some(registry_public.as_str())
        );
        assert_eq!(receipt.active.publisher.as_deref(), Some("Alice"));
        assert_eq!(
            receipt.active.publisher_key.as_deref(),
            Some(public_key_fingerprint(&publisher_public).unwrap().as_str())
        );
        assert_eq!(
            receipt.active.publisher_public_key.as_deref(),
            Some(publisher_public.as_str())
        );

        let on_disk = catalog.receipt(&receipt.id).unwrap();
        assert_eq!(on_disk.active.source_id.as_deref(), Some("community"));
        assert_eq!(
            on_disk.active.source_key.as_deref(),
            Some(registry_public.as_str())
        );
        assert_eq!(
            on_disk.active.publisher_key.as_deref(),
            Some(public_key_fingerprint(&publisher_public).unwrap().as_str())
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn extension_offered_by_second_source_at_higher_version_does_not_appear_as_update() {
        let root = test_root("cross-source-update");
        let catalog = catalog(&root);
        let (pub_a_priv, pub_a_pub) = generate_signing_key().unwrap();
        let (reg_a_priv, reg_a_pub) = generate_signing_key().unwrap();
        let (pub_b_priv, pub_b_pub) = generate_signing_key().unwrap();
        let (reg_b_priv, reg_b_pub) = generate_signing_key().unwrap();

        let pkg_v1 = package(&root, "1.0.0", "");
        sign_package(&pkg_v1, "Alice", &pub_a_priv).unwrap();
        let mut rel_v1 = RegistryRelease {
            id: ExtensionId::new("io.github.shilpo.catalog-test").unwrap(),
            name: "Catalog Test".into(),
            description: Some("Source A release".into()),
            publisher: "Alice".into(),
            version: Version::parse("1.0.0").unwrap(),
            api_version: Version::parse(SUPPORTED_API_VERSION).unwrap(),
            min_shilpo_version: Version::parse(CURRENT_SHILPO_VERSION).unwrap(),
            channel: ReleaseChannel::Stable,
            package_url: pkg_v1.display().to_string(),
            package_hash: hash_file(&pkg_v1).unwrap(),
            publisher_public_key: pub_a_pub,
            publisher_signature: String::new(),
            capabilities_hash: String::new(),
            capabilities: Vec::new(),
            published_at: "2026-07-26T12:00:00Z".into(),
            yanked: false,
            verified_publisher: true,
            open_source: true,
            data_only: true,
            key_rotation: None,
        };
        sign_release(&mut rel_v1, &pub_a_priv).unwrap();
        let index_a = sign_registry_index(
            RegistryIndex {
                schema_version: REGISTRY_SCHEMA_VERSION,
                source_id: "source-a".into(),
                generated_at: "2026-07-26T12:00:00Z".into(),
                releases: vec![rel_v1],
            },
            &reg_a_priv,
        )
        .unwrap();
        catalog
            .add_source(RegistrySource {
                id: "source-a".into(),
                name: "Source A".into(),
                index_url: root.join("source-a.json").display().to_string(),
                root_public_key: reg_a_pub,
                official: false,
                enabled: true,
            })
            .unwrap();
        catalog
            .store_index_bytes("source-a", &serde_json::to_vec(&index_a).unwrap())
            .unwrap();

        let installed = catalog
            .install_from_catalog(&ExtensionId::new("io.github.shilpo.catalog-test").unwrap())
            .unwrap();
        assert_eq!(installed.active.version, Version::parse("1.0.0").unwrap());
        assert_eq!(installed.active.source_id.as_deref(), Some("source-a"));

        let pkg_v2 = package(&root, "2.0.0", "");
        sign_package(&pkg_v2, "Bob", &pub_b_priv).unwrap();
        let mut rel_v2 = RegistryRelease {
            id: ExtensionId::new("io.github.shilpo.catalog-test").unwrap(),
            name: "Catalog Test".into(),
            description: Some("Source B release".into()),
            publisher: "Bob".into(),
            version: Version::parse("2.0.0").unwrap(),
            api_version: Version::parse(SUPPORTED_API_VERSION).unwrap(),
            min_shilpo_version: Version::parse(CURRENT_SHILPO_VERSION).unwrap(),
            channel: ReleaseChannel::Stable,
            package_url: pkg_v2.display().to_string(),
            package_hash: hash_file(&pkg_v2).unwrap(),
            publisher_public_key: pub_b_pub,
            publisher_signature: String::new(),
            capabilities_hash: String::new(),
            capabilities: Vec::new(),
            published_at: "2026-07-27T12:00:00Z".into(),
            yanked: false,
            verified_publisher: true,
            open_source: true,
            data_only: true,
            key_rotation: None,
        };
        sign_release(&mut rel_v2, &pub_b_priv).unwrap();
        let index_b = sign_registry_index(
            RegistryIndex {
                schema_version: REGISTRY_SCHEMA_VERSION,
                source_id: "source-b".into(),
                generated_at: "2026-07-27T12:00:00Z".into(),
                releases: vec![rel_v2],
            },
            &reg_b_priv,
        )
        .unwrap();
        catalog
            .add_source(RegistrySource {
                id: "source-b".into(),
                name: "Source B".into(),
                index_url: root.join("source-b.json").display().to_string(),
                root_public_key: reg_b_pub,
                official: false,
                enabled: true,
            })
            .unwrap();
        catalog
            .store_index_bytes("source-b", &serde_json::to_vec(&index_b).unwrap())
            .unwrap();

        let snapshot = catalog.snapshot();
        assert_eq!(snapshot.updates.len(), 1);
        assert_eq!(snapshot.updates[0].state, UpdateState::PublisherConflict);
        assert!(snapshot.updates[0].available.is_none());

        assert!(catalog.install_from_catalog(&installed.id).is_err());
        assert_eq!(
            catalog.receipt(&installed.id).unwrap().active.version,
            Version::parse("1.0.0").unwrap()
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn second_source_cannot_replace_or_shadow_installed_extension_package_grants_or_secrets() {
        let root = test_root("shadow-defense");
        let catalog = catalog(&root);
        let (pub_a_priv, pub_a_pub) = generate_signing_key().unwrap();
        let (reg_a_priv, reg_a_pub) = generate_signing_key().unwrap();
        let (pub_b_priv, _) = generate_signing_key().unwrap();

        let pkg_v1 = package(
            &root,
            "1.0.0",
            r#"
[[capabilities]]
kind = "notifications:show"
"#,
        );
        sign_package(&pkg_v1, "Alice", &pub_a_priv).unwrap();
        let mut rel_v1 = RegistryRelease {
            id: ExtensionId::new("io.github.shilpo.catalog-test").unwrap(),
            name: "Catalog Test".into(),
            description: Some("Legit release".into()),
            publisher: "Alice".into(),
            version: Version::parse("1.0.0").unwrap(),
            api_version: Version::parse(SUPPORTED_API_VERSION).unwrap(),
            min_shilpo_version: Version::parse(CURRENT_SHILPO_VERSION).unwrap(),
            channel: ReleaseChannel::Stable,
            package_url: pkg_v1.display().to_string(),
            package_hash: hash_file(&pkg_v1).unwrap(),
            publisher_public_key: pub_a_pub,
            publisher_signature: String::new(),
            capabilities_hash: String::new(),
            capabilities: vec![Capability::NotificationsShow],
            published_at: "2026-07-26T12:00:00Z".into(),
            yanked: false,
            verified_publisher: true,
            open_source: true,
            data_only: true,
            key_rotation: None,
        };
        sign_release(&mut rel_v1, &pub_a_priv).unwrap();
        let index_a = sign_registry_index(
            RegistryIndex {
                schema_version: REGISTRY_SCHEMA_VERSION,
                source_id: "trusted-source".into(),
                generated_at: "2026-07-26T12:00:00Z".into(),
                releases: vec![rel_v1],
            },
            &reg_a_priv,
        )
        .unwrap();
        catalog
            .add_source(RegistrySource {
                id: "trusted-source".into(),
                name: "Trusted Source".into(),
                index_url: root.join("trusted.json").display().to_string(),
                root_public_key: reg_a_pub,
                official: false,
                enabled: true,
            })
            .unwrap();
        catalog
            .store_index_bytes("trusted-source", &serde_json::to_vec(&index_a).unwrap())
            .unwrap();

        let installed = catalog
            .install_from_catalog(&ExtensionId::new("io.github.shilpo.catalog-test").unwrap())
            .unwrap();
        catalog
            .approve_capabilities(&installed.id, vec![Capability::NotificationsShow])
            .unwrap();
        catalog.set_enabled(&installed.id, true).unwrap();

        let broker = crate::secrets::FakeSecretBroker::new();
        let purpose = SecretPurpose::parse("auth-token").unwrap();
        let deadline = Instant::now() + Duration::from_secs(10);
        let secret_ref = broker
            .set(&installed.id, &purpose, b"alice-secret-token", deadline)
            .unwrap();

        let pkg_v2 = package(&root, "2.0.0", "");
        sign_package(&pkg_v2, "Mallory", &pub_b_priv).unwrap();

        assert!(matches!(
            catalog.install_local(&pkg_v2),
            Err(CatalogError::PublisherConflict(_))
        ));

        let receipt_after = catalog.receipt(&installed.id).unwrap();
        assert_eq!(
            receipt_after.active.version,
            Version::parse("1.0.0").unwrap()
        );
        assert_eq!(
            receipt_after.active.source_id.as_deref(),
            Some("trusted-source")
        );

        let grants = catalog.load_grants(&installed.id).unwrap();
        assert!(grants.enabled);
        assert_eq!(
            grants.granted_capabilities,
            vec![Capability::NotificationsShow]
        );

        let secret = broker
            .read(&installed.id, &purpose, &secret_ref, deadline)
            .unwrap();
        assert_eq!(secret.as_deref(), Some(&b"alice-secret-token"[..]));

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn same_id_offered_by_two_sources_is_reported_as_conflict_and_not_silently_resolved() {
        let root = test_root("discover-conflict");
        let catalog = catalog(&root);
        let (pub_a_priv, pub_a_pub) = generate_signing_key().unwrap();
        let (reg_a_priv, reg_a_pub) = generate_signing_key().unwrap();
        let (pub_b_priv, pub_b_pub) = generate_signing_key().unwrap();
        let (reg_b_priv, reg_b_pub) = generate_signing_key().unwrap();

        let pkg_a = package(&root, "1.0.0", "");
        sign_package(&pkg_a, "Alice", &pub_a_priv).unwrap();
        let mut rel_a = RegistryRelease {
            id: ExtensionId::new("io.github.shilpo.catalog-test").unwrap(),
            name: "Catalog Test A".into(),
            description: Some("From source A".into()),
            publisher: "Alice".into(),
            version: Version::parse("1.0.0").unwrap(),
            api_version: Version::parse(SUPPORTED_API_VERSION).unwrap(),
            min_shilpo_version: Version::parse(CURRENT_SHILPO_VERSION).unwrap(),
            channel: ReleaseChannel::Stable,
            package_url: pkg_a.display().to_string(),
            package_hash: hash_file(&pkg_a).unwrap(),
            publisher_public_key: pub_a_pub,
            publisher_signature: String::new(),
            capabilities_hash: String::new(),
            capabilities: Vec::new(),
            published_at: "2026-07-26T12:00:00Z".into(),
            yanked: false,
            verified_publisher: true,
            open_source: true,
            data_only: true,
            key_rotation: None,
        };
        sign_release(&mut rel_a, &pub_a_priv).unwrap();
        let index_a = sign_registry_index(
            RegistryIndex {
                schema_version: REGISTRY_SCHEMA_VERSION,
                source_id: "source-a".into(),
                generated_at: "2026-07-26T12:00:00Z".into(),
                releases: vec![rel_a],
            },
            &reg_a_priv,
        )
        .unwrap();
        catalog
            .add_source(RegistrySource {
                id: "source-a".into(),
                name: "Source A".into(),
                index_url: root.join("source-a.json").display().to_string(),
                root_public_key: reg_a_pub,
                official: false,
                enabled: true,
            })
            .unwrap();
        catalog
            .store_index_bytes("source-a", &serde_json::to_vec(&index_a).unwrap())
            .unwrap();

        let pkg_b = package(&root, "2.0.0", "");
        sign_package(&pkg_b, "Bob", &pub_b_priv).unwrap();
        let mut rel_b = RegistryRelease {
            id: ExtensionId::new("io.github.shilpo.catalog-test").unwrap(),
            name: "Catalog Test B".into(),
            description: Some("From source B".into()),
            publisher: "Bob".into(),
            version: Version::parse("2.0.0").unwrap(),
            api_version: Version::parse(SUPPORTED_API_VERSION).unwrap(),
            min_shilpo_version: Version::parse(CURRENT_SHILPO_VERSION).unwrap(),
            channel: ReleaseChannel::Stable,
            package_url: pkg_b.display().to_string(),
            package_hash: hash_file(&pkg_b).unwrap(),
            publisher_public_key: pub_b_pub,
            publisher_signature: String::new(),
            capabilities_hash: String::new(),
            capabilities: Vec::new(),
            published_at: "2026-07-27T12:00:00Z".into(),
            yanked: false,
            verified_publisher: true,
            open_source: true,
            data_only: true,
            key_rotation: None,
        };
        sign_release(&mut rel_b, &pub_b_priv).unwrap();
        let index_b = sign_registry_index(
            RegistryIndex {
                schema_version: REGISTRY_SCHEMA_VERSION,
                source_id: "source-b".into(),
                generated_at: "2026-07-27T12:00:00Z".into(),
                releases: vec![rel_b],
            },
            &reg_b_priv,
        )
        .unwrap();
        catalog
            .add_source(RegistrySource {
                id: "source-b".into(),
                name: "Source B".into(),
                index_url: root.join("source-b.json").display().to_string(),
                root_public_key: reg_b_pub,
                official: false,
                enabled: true,
            })
            .unwrap();
        catalog
            .store_index_bytes("source-b", &serde_json::to_vec(&index_b).unwrap())
            .unwrap();

        let snapshot = catalog.snapshot();
        assert_eq!(snapshot.discover.len(), 1);
        assert!(snapshot.discover[0].publisher_conflict);

        assert!(matches!(
            catalog
                .install_from_catalog(&ExtensionId::new("io.github.shilpo.catalog-test").unwrap()),
            Err(CatalogError::NotFound(_))
        ));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn adding_source_with_existing_id_fails_and_leaves_original_intact() {
        let root = test_root("source-reservation");
        let catalog = catalog(&root);
        let (_, reg_1_pub) = generate_signing_key().unwrap();
        let (_, reg_2_pub) = generate_signing_key().unwrap();

        catalog
            .add_source(RegistrySource {
                id: "community".into(),
                name: "Community One".into(),
                index_url: "https://community1.org/index.json".into(),
                root_public_key: reg_1_pub.clone(),
                official: false,
                enabled: true,
            })
            .unwrap();

        let initial_sources = catalog.sources().unwrap();
        assert_eq!(initial_sources.len(), 1);
        assert_eq!(initial_sources[0].name, "Community One");
        assert_eq!(initial_sources[0].root_public_key, reg_1_pub);

        let duplicate_result = catalog.add_source(RegistrySource {
            id: "community".into(),
            name: "Community Two".into(),
            index_url: "https://community2.org/index.json".into(),
            root_public_key: reg_2_pub,
            official: false,
            enabled: true,
        });

        assert!(matches!(
            duplicate_result,
            Err(CatalogError::InvalidRegistry(_))
        ));

        let sources_after = catalog.sources().unwrap();
        assert_eq!(sources_after.len(), 1);
        assert_eq!(sources_after[0].name, "Community One");
        assert_eq!(sources_after[0].root_public_key, reg_1_pub);
        assert_eq!(
            sources_after[0].index_url,
            "https://community1.org/index.json"
        );

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn source_switching_is_explicit_and_does_not_carry_grants_across_publishers() {
        let root = test_root("source-switch");
        let catalog = catalog(&root);
        let (pub_a_priv, pub_a_pub) = generate_signing_key().unwrap();
        let (reg_a_priv, reg_a_pub) = generate_signing_key().unwrap();
        let (pub_b_priv, pub_b_pub) = generate_signing_key().unwrap();
        let (reg_b_priv, reg_b_pub) = generate_signing_key().unwrap();

        let pkg_a = package(
            &root,
            "1.0.0",
            r#"
[[capabilities]]
kind = "notifications:show"
"#,
        );
        sign_package(&pkg_a, "Alice", &pub_a_priv).unwrap();
        let mut rel_a = RegistryRelease {
            id: ExtensionId::new("io.github.shilpo.catalog-test").unwrap(),
            name: "Catalog Test".into(),
            description: Some("Source A".into()),
            publisher: "Alice".into(),
            version: Version::parse("1.0.0").unwrap(),
            api_version: Version::parse(SUPPORTED_API_VERSION).unwrap(),
            min_shilpo_version: Version::parse(CURRENT_SHILPO_VERSION).unwrap(),
            channel: ReleaseChannel::Stable,
            package_url: pkg_a.display().to_string(),
            package_hash: hash_file(&pkg_a).unwrap(),
            publisher_public_key: pub_a_pub,
            publisher_signature: String::new(),
            capabilities_hash: String::new(),
            capabilities: vec![Capability::NotificationsShow],
            published_at: "2026-07-26T12:00:00Z".into(),
            yanked: false,
            verified_publisher: true,
            open_source: true,
            data_only: true,
            key_rotation: None,
        };
        sign_release(&mut rel_a, &pub_a_priv).unwrap();
        let index_a = sign_registry_index(
            RegistryIndex {
                schema_version: REGISTRY_SCHEMA_VERSION,
                source_id: "source-a".into(),
                generated_at: "2026-07-26T12:00:00Z".into(),
                releases: vec![rel_a],
            },
            &reg_a_priv,
        )
        .unwrap();
        catalog
            .add_source(RegistrySource {
                id: "source-a".into(),
                name: "Source A".into(),
                index_url: root.join("source-a.json").display().to_string(),
                root_public_key: reg_a_pub,
                official: false,
                enabled: true,
            })
            .unwrap();
        catalog
            .store_index_bytes("source-a", &serde_json::to_vec(&index_a).unwrap())
            .unwrap();

        let installed = catalog
            .install_from_catalog(&ExtensionId::new("io.github.shilpo.catalog-test").unwrap())
            .unwrap();
        catalog
            .approve_capabilities(&installed.id, vec![Capability::NotificationsShow])
            .unwrap();
        catalog.set_enabled(&installed.id, true).unwrap();

        let broker = crate::secrets::FakeSecretBroker::new();
        let purpose = SecretPurpose::parse("auth-token").unwrap();
        let deadline = Instant::now() + Duration::from_secs(10);
        let secret_ref = broker
            .set(&installed.id, &purpose, b"alice-secret", deadline)
            .unwrap();

        let pkg_b = package(
            &root,
            "2.0.0",
            r#"
[[capabilities]]
kind = "notifications:show"
"#,
        );
        sign_package(&pkg_b, "Bob", &pub_b_priv).unwrap();
        let mut rel_b = RegistryRelease {
            id: ExtensionId::new("io.github.shilpo.catalog-test").unwrap(),
            name: "Catalog Test".into(),
            description: Some("Source B".into()),
            publisher: "Bob".into(),
            version: Version::parse("2.0.0").unwrap(),
            api_version: Version::parse(SUPPORTED_API_VERSION).unwrap(),
            min_shilpo_version: Version::parse(CURRENT_SHILPO_VERSION).unwrap(),
            channel: ReleaseChannel::Stable,
            package_url: pkg_b.display().to_string(),
            package_hash: hash_file(&pkg_b).unwrap(),
            publisher_public_key: pub_b_pub.clone(),
            publisher_signature: String::new(),
            capabilities_hash: String::new(),
            capabilities: vec![Capability::NotificationsShow],
            published_at: "2026-07-27T12:00:00Z".into(),
            yanked: false,
            verified_publisher: true,
            open_source: true,
            data_only: true,
            key_rotation: None,
        };
        sign_release(&mut rel_b, &pub_b_priv).unwrap();
        let index_b = sign_registry_index(
            RegistryIndex {
                schema_version: REGISTRY_SCHEMA_VERSION,
                source_id: "source-b".into(),
                generated_at: "2026-07-27T12:00:00Z".into(),
                releases: vec![rel_b],
            },
            &reg_b_priv,
        )
        .unwrap();
        catalog
            .add_source(RegistrySource {
                id: "source-b".into(),
                name: "Source B".into(),
                index_url: root.join("source-b.json").display().to_string(),
                root_public_key: reg_b_pub.clone(),
                official: false,
                enabled: true,
            })
            .unwrap();
        catalog
            .store_index_bytes("source-b", &serde_json::to_vec(&index_b).unwrap())
            .unwrap();

        let switched = catalog
            .switch_source_with_secrets_policy(
                &installed.id,
                "source-b",
                SecretPolicy::Delete,
                Some(&broker),
            )
            .unwrap();

        assert_eq!(switched.active.version, Version::parse("2.0.0").unwrap());
        assert_eq!(switched.active.source_id.as_deref(), Some("source-b"));
        assert_eq!(
            switched.active.source_key.as_deref(),
            Some(reg_b_pub.as_str())
        );
        assert_eq!(switched.active.publisher.as_deref(), Some("Bob"));
        assert_eq!(
            switched.active.publisher_public_key.as_deref(),
            Some(pub_b_pub.as_str())
        );
        assert!(switched.previous.is_none());

        let grants_after = catalog.load_grants(&installed.id).unwrap();
        assert!(!grants_after.enabled);
        assert!(grants_after.granted_capabilities.is_empty());

        assert_eq!(
            broker
                .read(&installed.id, &purpose, &secret_ref, deadline)
                .unwrap(),
            None
        );

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn failed_source_switch_leaves_secrets_grants_and_receipt_untouched() {
        let root = test_root("source-switch-failure");
        let catalog = catalog(&root);
        let (pub_a_priv, pub_a_pub) = generate_signing_key().unwrap();
        let (reg_a_priv, reg_a_pub) = generate_signing_key().unwrap();
        let (pub_b_priv, pub_b_pub) = generate_signing_key().unwrap();
        let (reg_b_priv, reg_b_pub) = generate_signing_key().unwrap();

        let pkg_a = package(
            &root,
            "1.0.0",
            r#"
[[capabilities]]
kind = "notifications:show"
"#,
        );
        sign_package(&pkg_a, "Alice", &pub_a_priv).unwrap();
        let mut rel_a = RegistryRelease {
            id: ExtensionId::new("io.github.shilpo.catalog-test").unwrap(),
            name: "Catalog Test".into(),
            description: Some("Source A".into()),
            publisher: "Alice".into(),
            version: Version::parse("1.0.0").unwrap(),
            api_version: Version::parse(SUPPORTED_API_VERSION).unwrap(),
            min_shilpo_version: Version::parse(CURRENT_SHILPO_VERSION).unwrap(),
            channel: ReleaseChannel::Stable,
            package_url: pkg_a.display().to_string(),
            package_hash: hash_file(&pkg_a).unwrap(),
            publisher_public_key: pub_a_pub,
            publisher_signature: String::new(),
            capabilities_hash: String::new(),
            capabilities: vec![Capability::NotificationsShow],
            published_at: "2026-07-26T12:00:00Z".into(),
            yanked: false,
            verified_publisher: true,
            open_source: true,
            data_only: true,
            key_rotation: None,
        };
        sign_release(&mut rel_a, &pub_a_priv).unwrap();
        let index_a = sign_registry_index(
            RegistryIndex {
                schema_version: REGISTRY_SCHEMA_VERSION,
                source_id: "source-a".into(),
                generated_at: "2026-07-26T12:00:00Z".into(),
                releases: vec![rel_a],
            },
            &reg_a_priv,
        )
        .unwrap();
        catalog
            .add_source(RegistrySource {
                id: "source-a".into(),
                name: "Source A".into(),
                index_url: root.join("source-a.json").display().to_string(),
                root_public_key: reg_a_pub,
                official: false,
                enabled: true,
            })
            .unwrap();
        catalog
            .store_index_bytes("source-a", &serde_json::to_vec(&index_a).unwrap())
            .unwrap();

        let installed = catalog
            .install_from_catalog(&ExtensionId::new("io.github.shilpo.catalog-test").unwrap())
            .unwrap();
        catalog
            .approve_capabilities(&installed.id, vec![Capability::NotificationsShow])
            .unwrap();
        catalog.set_enabled(&installed.id, true).unwrap();

        let broker = crate::secrets::FakeSecretBroker::new();
        let purpose = SecretPurpose::parse("auth-token").unwrap();
        let deadline = Instant::now() + Duration::from_secs(10);
        let secret_ref = broker
            .set(&installed.id, &purpose, b"alice-secret", deadline)
            .unwrap();

        // Source B's release declares a package_hash that does not match the bytes at
        // its package_url. install_package_internal must reject this once it actually
        // hashes the fetched package, well after the switch has already committed to
        // a different publisher.
        let pkg_b = package(&root, "2.0.0", "");
        sign_package(&pkg_b, "Bob", &pub_b_priv).unwrap();
        let mut rel_b = RegistryRelease {
            id: ExtensionId::new("io.github.shilpo.catalog-test").unwrap(),
            name: "Catalog Test".into(),
            description: Some("Source B".into()),
            publisher: "Bob".into(),
            version: Version::parse("2.0.0").unwrap(),
            api_version: Version::parse(SUPPORTED_API_VERSION).unwrap(),
            min_shilpo_version: Version::parse(CURRENT_SHILPO_VERSION).unwrap(),
            channel: ReleaseChannel::Stable,
            package_url: pkg_b.display().to_string(),
            package_hash: "sha256:0000000000000000000000000000000000000000000000000000000000000000"
                .into(),
            publisher_public_key: pub_b_pub,
            publisher_signature: String::new(),
            capabilities_hash: String::new(),
            capabilities: vec![Capability::NotificationsShow],
            published_at: "2026-07-27T12:00:00Z".into(),
            yanked: false,
            verified_publisher: true,
            open_source: true,
            data_only: true,
            key_rotation: None,
        };
        sign_release(&mut rel_b, &pub_b_priv).unwrap();
        let index_b = sign_registry_index(
            RegistryIndex {
                schema_version: REGISTRY_SCHEMA_VERSION,
                source_id: "source-b".into(),
                generated_at: "2026-07-27T12:00:00Z".into(),
                releases: vec![rel_b],
            },
            &reg_b_priv,
        )
        .unwrap();
        catalog
            .add_source(RegistrySource {
                id: "source-b".into(),
                name: "Source B".into(),
                index_url: root.join("source-b.json").display().to_string(),
                root_public_key: reg_b_pub,
                official: false,
                enabled: true,
            })
            .unwrap();
        catalog
            .store_index_bytes("source-b", &serde_json::to_vec(&index_b).unwrap())
            .unwrap();

        let switch_result = catalog.switch_source_with_secrets_policy(
            &installed.id,
            "source-b",
            SecretPolicy::Delete,
            Some(&broker),
        );
        assert!(switch_result.is_err());

        let receipt_after = catalog.receipt(&installed.id).unwrap();
        assert_eq!(
            receipt_after.active.version,
            Version::parse("1.0.0").unwrap()
        );
        assert_eq!(receipt_after.active.source_id.as_deref(), Some("source-a"));

        let grants_after = catalog.load_grants(&installed.id).unwrap();
        assert!(grants_after.enabled);
        assert_eq!(
            grants_after.granted_capabilities,
            vec![Capability::NotificationsShow]
        );

        assert_eq!(
            broker
                .read(&installed.id, &purpose, &secret_ref, deadline)
                .unwrap()
                .as_deref(),
            Some(&b"alice-secret"[..])
        );

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn unsafe_archive_entries_are_rejected_without_replacing_current() {
        let root = test_root("unsafe-archive");
        let catalog = catalog(&root);
        let first = catalog.install_local(&package(&root, "1.0.0", "")).unwrap();
        let unsafe_package = root.join("unsafe.shilpo-ext");
        let output = File::create(&unsafe_package).unwrap();
        let mut builder = Builder::new(GzEncoder::new(output, Compression::default()));
        let mut header = Header::new_gnu();
        header.set_entry_type(tar::EntryType::Symlink);
        header.set_size(0);
        header.set_cksum();
        builder
            .append_link(&mut header, "extension.toml", "/etc/passwd")
            .unwrap();
        builder.into_inner().unwrap().finish().unwrap();
        assert!(matches!(
            catalog.install_local(&unsafe_package),
            Err(CatalogError::InvalidPackage(_))
        ));
        assert_eq!(
            catalog.receipt(&first.id).unwrap().active.version,
            Version::parse("1.0.0").unwrap()
        );
        fs::remove_dir_all(root).unwrap();
    }
}
