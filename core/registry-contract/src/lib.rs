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
pub const OFFICIAL_SOURCE_URL: &str =
    "https://raw.githubusercontent.com/shilpo-rs/extensions/main/index.json";

/// The compiled-in Ed25519 root public key for the official Shilpo extension registry.
/// Can be overridden at compile time via `SHILPO_OFFICIAL_EXTENSIONS_ROOT_KEY`.
///
/// This is the public half of `INDEX_SIGNING_KEY`, the private key held as a protected
/// GitHub Actions environment secret (`production`) in `shilpo-rs/extensions`, generated
/// via `shilpo ext keygen`. A public key is not a secret, so it belongs here verbatim,
/// matching ADR-0018 decision 6.
pub const OFFICIAL_ROOT_PUBLIC_KEY: &str = "j8Xk5rdfxsNG95pHDdChPrGnAG0NAPBBxAxUopOdA0w=";

/// Test-only override for [`RegistrySource::is_pinned_official`], simulating a build with
/// `SHILPO_OFFICIAL_EXTENSIONS_ROOT_KEY` compiled in. Gated behind the `test-util` feature,
/// which must only ever be enabled by a `[dev-dependencies]` edge — never by a normal
/// dependency — so this override compiles out of every real build, including release
/// builds of the `shilpo` binary. Without that gate, any code sharing this crate's public
/// API could call the setter below to forge official trust status at runtime.
#[cfg(feature = "test-util")]
#[doc(hidden)]
pub static TEST_OFFICIAL_ROOT_KEY: std::sync::RwLock<Option<String>> = std::sync::RwLock::new(None);

#[cfg(feature = "test-util")]
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
        #[cfg(feature = "test-util")]
        if let Ok(guard) = TEST_OFFICIAL_ROOT_KEY.read()
            && let Some(key) = guard.as_ref()
        {
            return self.id == OFFICIAL_SOURCE_ID && self.root_public_key == *key;
        }
        let expected_key = option_env!("SHILPO_OFFICIAL_EXTENSIONS_ROOT_KEY")
            .filter(|k| !k.trim().is_empty())
            .unwrap_or(OFFICIAL_ROOT_PUBLIC_KEY);
        self.id == OFFICIAL_SOURCE_ID && self.root_public_key == expected_key
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
    /// Monotonically increasing per source. The publisher increments this on every signed
    /// index it produces. `#[serde(default)]` so indexes published before this field existed
    /// deserialize as counter 0 rather than failing — they are the oldest thing any client
    /// could have cached, so treating them as the baseline is correct.
    #[serde(default)]
    pub counter: u64,
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
        .map_err(|_| index_signature_error(&source.id))?;
    verify_releases(&signed.index.releases)
}

/// Verifies a signed index straight from the bytes it was fetched as (network response or
/// cached file), without going through [`SignedRegistryIndex`]'s typed round-trip first.
///
/// [`verify_registry_index`] re-serializes the *parsed* index to reconstruct what it checks
/// the signature against. That reconstruction is only byte-identical to what was actually
/// signed as long as `RegistryIndex`'s fields never change -- add a field (even one that's
/// `#[serde(default)]`, like `counter` was), and every index signed before that field existed
/// fails verification retroactively, because the reconstructed JSON now includes a key the
/// original signer never saw. Verifying against the exact bytes the signer produced, located
/// via `RawValue` rather than re-derived through a struct, is immune to that class of schema
/// evolution by construction.
pub fn verify_registry_index_bytes(
    source: &RegistrySource,
    bytes: &[u8],
) -> Result<SignedRegistryIndex, ContractError> {
    #[derive(Deserialize)]
    struct RawEnvelope<'a> {
        #[serde(borrow)]
        index: &'a serde_json::value::RawValue,
        signature: String,
    }

    let envelope: RawEnvelope = serde_json::from_slice(bytes)
        .map_err(|error| ContractError::InvalidRegistry(error.to_string()))?;
    let index: RegistryIndex = serde_json::from_str(envelope.index.get())
        .map_err(|error| ContractError::InvalidRegistry(error.to_string()))?;
    if index.schema_version != REGISTRY_SCHEMA_VERSION || index.source_id != source.id {
        return Err(ContractError::InvalidRegistry(
            "index schema or source identity mismatch".into(),
        ));
    }
    verify_signature(
        &source.root_public_key,
        envelope.index.get().as_bytes(),
        &envelope.signature,
    )
    .map_err(|_| index_signature_error(&source.id))?;
    verify_releases(&index.releases)?;
    Ok(SignedRegistryIndex {
        index,
        signature: envelope.signature,
    })
}

fn index_signature_error(source_id: &str) -> ContractError {
    ContractError::InvalidSignature(format!(
        "index for source '{source_id}' does not verify against its pinned key — the index may \
         be tampered, or the source's signing key changed; if you trust this is a deliberate key \
         change, remove and re-add the source to accept the new key"
    ))
}

fn verify_releases(releases: &[RegistryRelease]) -> Result<(), ContractError> {
    for release in releases {
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

/// A signed index that already verified (see [`verify_registry_index`]) is also checked
/// against whatever the client already had cached for that source, so a validly-signed but
/// stale index can't silently replace a newer one the client has already seen. `previous` is
/// `None` on first fetch for a source — anything is accepted as the baseline then.
///
/// | previous counter vs new | outcome |
/// |---|---|
/// | none cached yet | accept |
/// | new < previous | reject — rollback/replay |
/// | new == previous, identical payload | accept, no-op |
/// | new == previous, different payload | reject — same counter can't mean two different things |
/// | new > previous | accept |
pub fn verify_index_ordering(
    previous: Option<&SignedRegistryIndex>,
    new: &SignedRegistryIndex,
) -> Result<(), ContractError> {
    let Some(previous) = previous else {
        return Ok(());
    };
    match new.index.counter.cmp(&previous.index.counter) {
        std::cmp::Ordering::Less => Err(ContractError::InvalidRegistry(format!(
            "index for source '{}' has counter {} — an index with counter {} was already seen; \
             refusing an older or replayed index",
            new.index.source_id, new.index.counter, previous.index.counter
        ))),
        std::cmp::Ordering::Equal if new.index != previous.index => {
            Err(ContractError::InvalidRegistry(format!(
                "index for source '{}' repeats counter {} with different content — the source is \
                 either misbehaving or under attack",
                new.index.source_id, new.index.counter
            )))
        }
        _ => Ok(()),
    }
}

/// A generated_at older than this many days only produces a warning, never a rejection — a
/// legitimate static registry can go quiet for a long time without that being an attack. See
/// `verify_index_ordering` for the actual anti-replay mechanism, which does reject.
pub const STALE_INDEX_WARNING_DAYS: i64 = 180;

/// Returns a warning string if `generated_at` is older than [`STALE_INDEX_WARNING_DAYS`], or
/// if it fails to parse at all (itself worth surfacing, though not a rejection). `None` means
/// the index is fresh enough that nothing needs to be said.
pub fn stale_index_warning(source_id: &str, generated_at: &str) -> Option<String> {
    match chrono::DateTime::parse_from_rfc3339(generated_at) {
        Ok(timestamp) => {
            let age = chrono::Utc::now().signed_duration_since(timestamp);
            (age.num_days() > STALE_INDEX_WARNING_DAYS).then(|| {
                format!(
                    "index for source '{source_id}' was generated {} days ago — the source may \
                     be unmaintained or abandoned, not necessarily compromised",
                    age.num_days()
                )
            })
        }
        Err(_) => Some(format!(
            "index for source '{source_id}' has an unparseable generated_at value '{generated_at}'"
        )),
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_official_constants_and_pinning() {
        let official = RegistrySource {
            id: OFFICIAL_SOURCE_ID.into(),
            name: OFFICIAL_SOURCE_NAME.into(),
            index_url: OFFICIAL_SOURCE_URL.into(),
            root_public_key: OFFICIAL_ROOT_PUBLIC_KEY.into(),
            official: true,
            enabled: true,
        };
        assert!(official.is_pinned_official());

        let fake_key = RegistrySource {
            id: OFFICIAL_SOURCE_ID.into(),
            name: OFFICIAL_SOURCE_NAME.into(),
            index_url: OFFICIAL_SOURCE_URL.into(),
            root_public_key: "fake-public-key".into(),
            official: true,
            enabled: true,
        };
        assert!(!fake_key.is_pinned_official());

        let fake_id = RegistrySource {
            id: "community".into(),
            name: "Community".into(),
            index_url: "https://example.com/index.json".into(),
            root_public_key: OFFICIAL_ROOT_PUBLIC_KEY.into(),
            official: true,
            enabled: true,
        };
        assert!(!fake_id.is_pinned_official());
    }

    #[test]
    fn test_signing_and_verification() {
        let (private_key, public_key) = generate_signing_key().unwrap();
        let message = b"hello world";
        let pair = decode_key_pair(&private_key).unwrap();
        let signature = BASE64.encode(pair.sign(message).as_ref());

        assert!(verify_signature(&public_key, message, &signature).is_ok());
        assert!(verify_signature(&public_key, b"different message", &signature).is_err());
    }
}
