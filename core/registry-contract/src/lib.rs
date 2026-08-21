use std::fmt;
use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64;
use ring::rand::SystemRandom;
use ring::signature::{self, Ed25519KeyPair, KeyPair};
use schemars::JsonSchema;
use semver::Version;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use shilpo_ext_api::{Capability, ExtensionId};

pub const REGISTRY_SCHEMA_VERSION: u32 = 1;

#[derive(Debug)]
pub enum ContractError {
    Io(String),
    InvalidSignature(String),
    InvalidRegistry(String),
}

impl fmt::Display for ContractError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(message) => write!(formatter, "I/O error: {message}"),
            Self::InvalidSignature(message) => write!(formatter, "invalid signature: {message}"),
            Self::InvalidRegistry(message) => write!(formatter, "invalid registry: {message}"),
        }
    }
}

impl std::error::Error for ContractError {}

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum ReleaseChannel {
    #[default]
    Stable,
    Beta,
    Development,
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

pub const OFFICIAL_SOURCE_ID: &str = "shilpo";
pub const OFFICIAL_SOURCE_NAME: &str = "Shilpo Extensions";
pub const OFFICIAL_SOURCE_URL: &str = "https://extensions.shilpo.org/index.json";

#[doc(hidden)]
pub static TEST_OFFICIAL_ROOT_KEY: std::sync::RwLock<Option<String>> = std::sync::RwLock::new(None);

#[doc(hidden)]
pub fn set_test_official_root_key(key: Option<String>) {
    let mut guard = TEST_OFFICIAL_ROOT_KEY.write().unwrap();
    *guard = key;
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

impl RegistrySource {
    pub fn is_pinned_official(&self) -> bool {
        if let Ok(guard) = TEST_OFFICIAL_ROOT_KEY.read()
            && let Some(key) = guard.as_ref()
        {
            return self.id == OFFICIAL_SOURCE_ID && self.root_public_key == *key;
        }
        option_env!("SHILPO_OFFICIAL_EXTENSIONS_ROOT_KEY")
            .filter(|k| !k.trim().is_empty())
            .is_some_and(|key| self.id == OFFICIAL_SOURCE_ID && self.root_public_key == key)
    }
}

fn default_true() -> bool {
    true
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
    pub official: bool,
    #[serde(default)]
    pub verified_publisher: bool,
    #[serde(default)]
    pub open_source: bool,
    #[serde(default)]
    pub data_only: bool,
    #[serde(default)]
    pub key_rotation: Option<KeyRotationDelegation>,
}

pub fn package_signature_path(package: &Path) -> PathBuf {
    PathBuf::from(format!("{}.sig.json", package.display()))
}

pub fn generate_signing_key() -> Result<(String, String), ContractError> {
    let document = Ed25519KeyPair::generate_pkcs8(&SystemRandom::new())
        .map_err(|_| ContractError::InvalidSignature("failed to generate Ed25519 key".into()))?;
    let pair = Ed25519KeyPair::from_pkcs8(document.as_ref())
        .map_err(|_| ContractError::InvalidSignature("generated key is invalid".into()))?;
    Ok((
        BASE64.encode(document.as_ref()),
        BASE64.encode(pair.public_key().as_ref()),
    ))
}

pub fn sign_package(
    package: &Path,
    publisher: &str,
    private_key: &str,
) -> Result<PathBuf, ContractError> {
    if publisher.trim().is_empty() {
        return Err(ContractError::InvalidSignature(
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
        .map_err(|error| ContractError::InvalidSignature(error.to_string()))?;
    write_atomic(&path, &bytes)?;
    Ok(path)
}

pub fn sign_release(release: &mut RegistryRelease, private_key: &str) -> Result<(), ContractError> {
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
) -> Result<SignedRegistryIndex, ContractError> {
    let pair = decode_key_pair(private_key)?;
    let payload = serde_json::to_vec(&index)
        .map_err(|error| ContractError::InvalidRegistry(error.to_string()))?;
    Ok(SignedRegistryIndex {
        index,
        signature: BASE64.encode(pair.sign(&payload).as_ref()),
    })
}

pub fn decode_key_pair(private_key: &str) -> Result<Ed25519KeyPair, ContractError> {
    let bytes = BASE64
        .decode(private_key.trim())
        .map_err(|error| ContractError::InvalidSignature(error.to_string()))?;
    Ed25519KeyPair::from_pkcs8(&bytes)
        .map_err(|_| ContractError::InvalidSignature("invalid Ed25519 private key".into()))
}

pub fn verify_signature(
    public_key: &str,
    message: &[u8],
    encoded_signature: &str,
) -> Result<(), ContractError> {
    let key = BASE64
        .decode(public_key)
        .map_err(|error| ContractError::InvalidSignature(error.to_string()))?;
    let signature = BASE64
        .decode(encoded_signature)
        .map_err(|error| ContractError::InvalidSignature(error.to_string()))?;
    signature::UnparsedPublicKey::new(&signature::ED25519, key)
        .verify(message, &signature)
        .map_err(|_| ContractError::InvalidSignature("Ed25519 verification failed".into()))
}

pub fn public_key_fingerprint(public_key: &str) -> Result<String, ContractError> {
    let bytes = BASE64
        .decode(public_key)
        .map_err(|error| ContractError::InvalidSignature(error.to_string()))?;
    Ok(hash_bytes(&bytes))
}

pub fn package_signing_message(publisher: &str, package_hash: &str) -> String {
    format!("shilpo-package-v1\n{publisher}\n{package_hash}")
}

pub fn rotation_message(next_public_key: &str) -> String {
    format!("shilpo-key-rotation-v1\n{next_public_key}")
}

pub fn release_signing_payload(release: &RegistryRelease) -> Result<Vec<u8>, ContractError> {
    let mut unsigned = release.clone();
    unsigned.publisher_signature.clear();
    serde_json::to_vec(&unsigned).map_err(|error| ContractError::InvalidRegistry(error.to_string()))
}

pub fn capabilities_hash(capabilities: &[Capability]) -> Result<String, ContractError> {
    let bytes = serde_json::to_vec(capabilities)
        .map_err(|error| ContractError::InvalidRegistry(error.to_string()))?;
    Ok(hash_bytes(&bytes))
}

pub fn verify_release_signature(release: &RegistryRelease) -> Result<(), ContractError> {
    let payload = release_signing_payload(release)?;
    verify_signature(
        &release.publisher_public_key,
        &payload,
        &release.publisher_signature,
    )
}

pub fn verify_registry_index(
    source: &RegistrySource,
    signed: &SignedRegistryIndex,
) -> Result<(), ContractError> {
    if signed.index.schema_version != REGISTRY_SCHEMA_VERSION || signed.index.source_id != source.id
    {
        return Err(ContractError::InvalidRegistry(
            "index schema or source identity mismatch".into(),
        ));
    }
    let payload = serde_json::to_vec(&signed.index)
        .map_err(|error| ContractError::InvalidRegistry(error.to_string()))?;
    verify_signature(&source.root_public_key, &payload, &signed.signature)
        .map_err(|error| ContractError::InvalidRegistry(error.to_string()))?;
    for release in &signed.index.releases {
        verify_release_signature(release)
            .map_err(|error| ContractError::InvalidRegistry(error.to_string()))?;
        if let Some(rotation) = &release.key_rotation {
            if rotation.next_public_key != release.publisher_public_key {
                return Err(ContractError::InvalidRegistry(format!(
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
                .map_err(|error| ContractError::InvalidRegistry(error.to_string()))?;
            }
        }
        if capabilities_hash(&release.capabilities)? != release.capabilities_hash {
            return Err(ContractError::InvalidRegistry(format!(
                "capability hash mismatch for '{}' {}",
                release.id, release.version
            )));
        }
    }
    Ok(())
}

pub fn validate_source(source: &RegistrySource) -> Result<(), ContractError> {
    if source.id.is_empty()
        || !source
            .id
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
        || source.name.trim().is_empty()
        || source.index_url.trim().is_empty()
    {
        return Err(ContractError::InvalidRegistry(
            "source ID, name, or URL is invalid".into(),
        ));
    }
    public_key_fingerprint(&source.root_public_key)?;
    Ok(())
}

pub fn hash_file(path: &Path) -> Result<String, ContractError> {
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

pub fn hash_bytes(bytes: &[u8]) -> String {
    format!("sha256:{}", hex::encode(Sha256::digest(bytes)))
}

fn io_error(path: &Path, error: std::io::Error) -> ContractError {
    ContractError::Io(format!("{}: {error}", path.display()))
}

fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), ContractError> {
    let parent = path
        .parent()
        .ok_or_else(|| ContractError::Io(format!("{} has no parent", path.display())))?;
    std::fs::create_dir_all(parent).map_err(|error| io_error(parent, error))?;
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
        std::fs::rename(&temporary, path).map_err(|error| io_error(path, error))?;
        if let Ok(directory) = File::open(parent) {
            let _ = directory.sync_all();
        }
        Ok(())
    })();
    if temporary.exists() {
        let _ = std::fs::remove_file(temporary);
    }
    result
}

fn unique_suffix() -> String {
    format!(
        "{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    )
}
