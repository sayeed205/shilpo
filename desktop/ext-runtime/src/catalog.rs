use crate::cli::ExtensionCli;
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64;
use flate2::read::GzDecoder;
use ring::rand::SystemRandom;
use ring::signature::{self, Ed25519KeyPair, KeyPair};
use schemars::JsonSchema;
use semver::Version;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use shilpo_ext_api::{Capability, ExtensionId, ExtensionManifest, SUPPORTED_API_VERSION};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use tar::Archive;

const MAX_PACKAGE_BYTES: u64 = 64 * 1024 * 1024;
const MAX_INDEX_BYTES: usize = 8 * 1024 * 1024;
const RECEIPT_SCHEMA_VERSION: u32 = 1;
const REGISTRY_SCHEMA_VERSION: u32 = 1;
pub const CURRENT_SHILPO_VERSION: &str = "0.1.0";
const OFFICIAL_SOURCE_ID: &str = "shilpo";
const OFFICIAL_SOURCE_NAME: &str = "Shilpo Extensions";
const OFFICIAL_SOURCE_URL: &str = "https://extensions.shilpo.org/index.json";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CatalogPaths {
    pub data_dir: PathBuf,
    pub config_dir: PathBuf,
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

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum ReleaseChannel {
    #[default]
    Stable,
    Beta,
    Development,
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

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PackageSignature {
    pub schema_version: u32,
    pub publisher: String,
    pub public_key: String,
    pub package_hash: String,
    pub signature: String,
    #[serde(default)]
    pub key_rotation: Option<KeyRotationDelegation>,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct KeyRotationDelegation {
    pub previous_public_key: String,
    pub next_public_key: String,
    pub signature: String,
    #[serde(default)]
    pub authorized_by_registry: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct RegistrySource {
    pub id: String,
    pub name: String,
    pub index_url: String,
    pub root_public_key: String,
    #[serde(default)]
    pub official: bool,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

fn default_true() -> bool {
    true
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct StoredSources {
    #[serde(default)]
    source: Vec<RegistrySource>,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct RegistryIndex {
    pub schema_version: u32,
    pub source_id: String,
    pub generated_at: String,
    #[serde(default)]
    pub releases: Vec<RegistryRelease>,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct SignedRegistryIndex {
    pub index: RegistryIndex,
    pub signature: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct RegistryRelease {
    pub id: ExtensionId,
    pub name: String,
    pub description: Option<String>,
    pub publisher: String,
    #[schemars(with = "String")]
    pub version: Version,
    #[schemars(with = "String")]
    pub api_version: Version,
    #[schemars(with = "String")]
    pub min_shilpo_version: Version,
    pub channel: ReleaseChannel,
    pub package_url: String,
    pub package_hash: String,
    pub publisher_public_key: String,
    pub publisher_signature: String,
    pub capabilities_hash: String,
    #[serde(default)]
    pub capabilities: Vec<Capability>,
    pub published_at: String,
    #[serde(default)]
    pub yanked: bool,
    #[serde(default)]
    pub verified_publisher: bool,
    #[serde(default)]
    pub open_source: bool,
    #[serde(default)]
    pub data_only: bool,
    #[serde(default)]
    pub key_rotation: Option<KeyRotationDelegation>,
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
        let receipt_path = self.receipt_path(extension_id);
        if !receipt_path.is_file() {
            return Err(CatalogError::NotFound(extension_id.to_string()));
        }
        let package_dir = self.extension_dir(extension_id);
        let trash_dir = self.paths.staging_dir().join(format!(
            "uninstall-{}-{}",
            extension_id,
            unique_suffix()
        ));
        fs::create_dir_all(self.paths.staging_dir())
            .map_err(|error| io_error(&self.paths.staging_dir(), error))?;
        if package_dir.exists() {
            fs::rename(&package_dir, &trash_dir).map_err(|error| io_error(&package_dir, error))?;
        }
        if let Err(error) = fs::remove_file(&receipt_path) {
            if trash_dir.exists() {
                let _ = fs::rename(&trash_dir, &package_dir);
            }
            return Err(io_error(&receipt_path, error));
        }
        let grants_path = self.grants_path(extension_id);
        if grants_path.exists() {
            let _ = fs::remove_file(grants_path);
        }
        if trash_dir.exists() {
            fs::remove_dir_all(&trash_dir).map_err(|error| io_error(&trash_dir, error))?;
        }
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
        if let Some(existing) = sources.iter_mut().find(|item| item.id == source.id) {
            *existing = source;
        } else {
            sources.push(source);
        }
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

    fn install_package(
        &self,
        package: &Path,
        provenance: PackageProvenance,
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
            verify_publisher_continuity(previous_receipt.as_ref(), &verified)?;
            let target = self.package_dir(&manifest.id, &manifest.version);
            if target.exists() {
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
            } else {
                let parent = target
                    .parent()
                    .expect("version directory always has a parent");
                fs::create_dir_all(parent).map_err(|error| io_error(parent, error))?;
                mark_files_read_only(&stage)?;
                fs::rename(&stage, &target).map_err(|error| io_error(&target, error))?;
            }

            let mut grants = self.load_grants(&manifest.id)?;
            let capability_expansion = previous_receipt.is_some()
                && manifest
                    .capabilities
                    .iter()
                    .any(|capability| !grants.granted_capabilities.contains(capability));
            let now = unix_timestamp();
            let installed_version = InstalledVersionReceipt {
                version: manifest.version.clone(),
                source: verified.source,
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
            if receipt.active.version != manifest.version {
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
            let conflict = has_publisher_conflict(&candidates);
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
                } else if newer
                    .iter()
                    .any(|candidate| !publisher_matches(receipt, candidate))
                {
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
        publisher: Some(package_signature.publisher),
        publisher_key: Some(fingerprint),
        publisher_public_key: Some(package_signature.public_key),
        trust: TrustState::SignedThirdParty,
        channel: ReleaseChannel::Stable,
        key_rotation: package_signature.key_rotation,
        expected_release: None,
    })
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

fn verify_registry_index(
    source: &RegistrySource,
    signed: &SignedRegistryIndex,
) -> Result<(), CatalogError> {
    if signed.index.schema_version != REGISTRY_SCHEMA_VERSION || signed.index.source_id != source.id
    {
        return Err(CatalogError::InvalidRegistry(
            "index schema or source identity mismatch".into(),
        ));
    }
    let payload = serde_json::to_vec(&signed.index)
        .map_err(|error| CatalogError::InvalidRegistry(error.to_string()))?;
    verify_signature(&source.root_public_key, &payload, &signed.signature)
        .map_err(|error| CatalogError::InvalidRegistry(error.to_string()))?;
    for release in &signed.index.releases {
        verify_release_signature(release)
            .map_err(|error| CatalogError::InvalidRegistry(error.to_string()))?;
        if let Some(rotation) = &release.key_rotation {
            if rotation.next_public_key != release.publisher_public_key {
                return Err(CatalogError::InvalidRegistry(format!(
                    "key rotation for '{}' does not end at the release key",
                    release.id
                )));
            }
            if !rotation.authorized_by_registry {
                verify_signature(
                    &rotation.previous_public_key,
                    rotation_message(&rotation.next_public_key).as_bytes(),
                    &rotation.signature,
                )
                .map_err(|error| CatalogError::InvalidRegistry(error.to_string()))?;
            }
        }
        if capabilities_hash(&release.capabilities)? != release.capabilities_hash {
            return Err(CatalogError::InvalidRegistry(format!(
                "capability hash mismatch for '{}' {}",
                release.id, release.version
            )));
        }
    }
    Ok(())
}

fn verify_release_signature(release: &RegistryRelease) -> Result<(), CatalogError> {
    let payload = release_signing_payload(release)?;
    verify_signature(
        &release.publisher_public_key,
        &payload,
        &release.publisher_signature,
    )
}

fn release_signing_payload(release: &RegistryRelease) -> Result<Vec<u8>, CatalogError> {
    let mut unsigned = release.clone();
    unsigned.publisher_signature.clear();
    serde_json::to_vec(&unsigned).map_err(|error| CatalogError::InvalidRegistry(error.to_string()))
}

fn capabilities_hash(capabilities: &[Capability]) -> Result<String, CatalogError> {
    let bytes = serde_json::to_vec(capabilities)
        .map_err(|error| CatalogError::InvalidRegistry(error.to_string()))?;
    Ok(hash_bytes(&bytes))
}

pub fn package_signature_path(package: &Path) -> PathBuf {
    PathBuf::from(format!("{}.sig.json", package.display()))
}

pub fn generate_signing_key() -> Result<(String, String), CatalogError> {
    let document = Ed25519KeyPair::generate_pkcs8(&SystemRandom::new())
        .map_err(|_| CatalogError::InvalidSignature("failed to generate Ed25519 key".into()))?;
    let pair = Ed25519KeyPair::from_pkcs8(document.as_ref())
        .map_err(|_| CatalogError::InvalidSignature("generated key is invalid".into()))?;
    Ok((
        BASE64.encode(document.as_ref()),
        BASE64.encode(pair.public_key().as_ref()),
    ))
}

pub fn sign_package(
    package: &Path,
    publisher: &str,
    private_key: &str,
) -> Result<PathBuf, CatalogError> {
    if publisher.trim().is_empty() {
        return Err(CatalogError::InvalidSignature(
            "publisher cannot be empty".into(),
        ));
    }
    let package_hash = hash_file(package)?;
    let pair = decode_key_pair(private_key)?;
    let signature = pair.sign(package_signing_message(publisher, &package_hash).as_bytes());
    let sidecar = PackageSignature {
        schema_version: 1,
        publisher: publisher.to_owned(),
        public_key: BASE64.encode(pair.public_key().as_ref()),
        package_hash,
        signature: BASE64.encode(signature.as_ref()),
        key_rotation: None,
    };
    let path = package_signature_path(package);
    let bytes = serde_json::to_vec_pretty(&sidecar)
        .map_err(|error| CatalogError::InvalidSignature(error.to_string()))?;
    write_atomic(&path, &bytes)?;
    Ok(path)
}

pub fn sign_release(release: &mut RegistryRelease, private_key: &str) -> Result<(), CatalogError> {
    let pair = decode_key_pair(private_key)?;
    release.publisher_public_key = BASE64.encode(pair.public_key().as_ref());
    release.capabilities_hash = capabilities_hash(&release.capabilities)?;
    release.publisher_signature.clear();
    let payload = release_signing_payload(release)?;
    release.publisher_signature = BASE64.encode(pair.sign(&payload).as_ref());
    Ok(())
}

pub fn sign_registry_index(
    index: RegistryIndex,
    private_key: &str,
) -> Result<SignedRegistryIndex, CatalogError> {
    let pair = decode_key_pair(private_key)?;
    let payload = serde_json::to_vec(&index)
        .map_err(|error| CatalogError::InvalidRegistry(error.to_string()))?;
    Ok(SignedRegistryIndex {
        index,
        signature: BASE64.encode(pair.sign(&payload).as_ref()),
    })
}

fn decode_key_pair(private_key: &str) -> Result<Ed25519KeyPair, CatalogError> {
    let bytes = BASE64
        .decode(private_key.trim())
        .map_err(|error| CatalogError::InvalidSignature(error.to_string()))?;
    Ed25519KeyPair::from_pkcs8(&bytes)
        .map_err(|_| CatalogError::InvalidSignature("invalid Ed25519 private key".into()))
}

fn verify_signature(
    public_key: &str,
    message: &[u8],
    encoded_signature: &str,
) -> Result<(), CatalogError> {
    let key = BASE64
        .decode(public_key)
        .map_err(|error| CatalogError::InvalidSignature(error.to_string()))?;
    let signature = BASE64
        .decode(encoded_signature)
        .map_err(|error| CatalogError::InvalidSignature(error.to_string()))?;
    signature::UnparsedPublicKey::new(&signature::ED25519, key)
        .verify(message, &signature)
        .map_err(|_| CatalogError::InvalidSignature("Ed25519 verification failed".into()))
}

fn public_key_fingerprint(public_key: &str) -> Result<String, CatalogError> {
    let bytes = BASE64
        .decode(public_key)
        .map_err(|error| CatalogError::InvalidSignature(error.to_string()))?;
    Ok(hash_bytes(&bytes))
}

fn package_signing_message(publisher: &str, package_hash: &str) -> String {
    format!("shilpo-package-v1\n{publisher}\n{package_hash}")
}

fn rotation_message(next_public_key: &str) -> String {
    format!("shilpo-key-rotation-v1\n{next_public_key}")
}

fn validate_source(source: &RegistrySource) -> Result<(), CatalogError> {
    if source.id.is_empty()
        || !source
            .id
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
        || source.name.trim().is_empty()
        || source.index_url.trim().is_empty()
    {
        return Err(CatalogError::InvalidRegistry(
            "source ID, name, or URL is invalid".into(),
        ));
    }
    public_key_fingerprint(&source.root_public_key)?;
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

fn hash_file(path: &Path) -> Result<String, CatalogError> {
    let mut file = File::open(path).map_err(|error| io_error(path, error))?;
    let mut digest = Sha256::new();
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let count = file
            .read(&mut buffer)
            .map_err(|error| io_error(path, error))?;
        if count == 0 {
            break;
        }
        digest.update(&buffer[..count]);
    }
    Ok(format!("sha256:{}", hex::encode(digest.finalize())))
}

fn hash_bytes(bytes: &[u8]) -> String {
    format!("sha256:{}", hex::encode(Sha256::digest(bytes)))
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
    use super::*;
    use flate2::Compression;
    use flate2::write::GzEncoder;
    use std::fs;
    use tar::{Builder, Header};

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
        let installed = catalog.install_local(&first_package).unwrap();
        let second_package = package(&root, "2.0.0", "");
        let mut release = RegistryRelease {
            id: installed.id.clone(),
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
                root_public_key: registry_public,
                official: false,
                enabled: true,
            })
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
