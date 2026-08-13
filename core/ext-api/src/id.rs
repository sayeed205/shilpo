use schemars::JsonSchema;
use serde::{Deserialize, Deserializer, Serialize, de};
use std::fmt;
use std::str::FromStr;

/// Scoped error type for identifier parsing and validation.
#[derive(Debug, PartialEq, Eq, Clone)]
pub enum IdError {
    InvalidExtensionId(String),
    InvalidContributionId(String),
    InvalidCanonicalId(String),
}

impl fmt::Display for IdError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidExtensionId(value) => write!(
                f,
                "invalid extension ID '{value}': expected lowercase reverse-domain segments"
            ),
            Self::InvalidContributionId(value) => write!(
                f,
                "invalid contribution ID '{value}': expected lowercase letters, digits, dashes, or underscores"
            ),
            Self::InvalidCanonicalId(value) => {
                write!(f, "invalid canonical contribution ID '{value}'")
            }
        }
    }
}

impl std::error::Error for IdError {}

/// The package identity of an extension (reverse-domain notation, e.g. `io.github.alice`).
#[derive(Clone, Debug, Serialize, JsonSchema, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[serde(transparent)]
pub struct ExtensionId(String);

impl ExtensionId {
    pub fn new(id: impl Into<String>) -> Result<Self, IdError> {
        let value = id.into();
        let segments = value.split('.').collect::<Vec<_>>();
        let valid = segments.len() >= 3
            && segments.iter().all(|segment| {
                !segment.is_empty()
                    && segment
                        .bytes()
                        .next()
                        .is_some_and(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit())
                    && segment.bytes().all(|byte| {
                        byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-'
                    })
            });
        if !valid {
            return Err(IdError::InvalidExtensionId(value));
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl<'de> Deserialize<'de> for ExtensionId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        Self::new(String::deserialize(deserializer)?).map_err(de::Error::custom)
    }
}

impl fmt::Display for ExtensionId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

/// A single-segment identifier naming one contribution within an extension (e.g. `clock`).
#[derive(Clone, Debug, Serialize, JsonSchema, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[serde(transparent)]
pub struct ContributionId(String);

impl ContributionId {
    pub fn new(id: impl Into<String>) -> Result<Self, IdError> {
        let value = id.into();
        let valid = value
            .bytes()
            .next()
            .is_some_and(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit())
            && value.bytes().all(|byte| {
                byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-' || byte == b'_'
            });
        if !valid {
            return Err(IdError::InvalidContributionId(value));
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl<'de> Deserialize<'de> for ContributionId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        Self::new(String::deserialize(deserializer)?).map_err(de::Error::custom)
    }
}

impl fmt::Display for ContributionId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

/// The composed, globally-unique address of a contribution (`extension/contribution`).
///
/// Lookups and addressing across the system are keyed on this canonical identifier.
#[derive(Clone, Debug, JsonSchema, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[schemars(with = "String")]
pub struct CanonicalId {
    pub extension_id: ExtensionId,
    pub contribution_id: ContributionId,
}

impl CanonicalId {
    pub fn new(extension_id: ExtensionId, contribution_id: ContributionId) -> Self {
        Self {
            extension_id,
            contribution_id,
        }
    }
}

impl FromStr for CanonicalId {
    type Err = IdError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let Some((extension_id, contribution_id)) = value.split_once('/') else {
            return Err(IdError::InvalidCanonicalId(value.to_owned()));
        };
        if contribution_id.contains('/') {
            return Err(IdError::InvalidCanonicalId(value.to_owned()));
        }
        Ok(Self::new(
            ExtensionId::new(extension_id)?,
            ContributionId::new(contribution_id)?,
        ))
    }
}

impl Serialize for CanonicalId {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for CanonicalId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        String::deserialize(deserializer)?
            .parse()
            .map_err(de::Error::custom)
    }
}

impl fmt::Display for CanonicalId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}/{}", self.extension_id, self.contribution_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_extension_id() {
        let id = ExtensionId::new("io.github.alice").unwrap();
        assert_eq!(id.as_str(), "io.github.alice");
        assert_eq!(id.to_string(), "io.github.alice");

        let id = ExtensionId::new("com.example.my-ext-1").unwrap();
        assert_eq!(id.as_str(), "com.example.my-ext-1");
    }

    #[test]
    fn test_invalid_extension_id() {
        assert!(matches!(
            ExtensionId::new("invalid"),
            Err(IdError::InvalidExtensionId(_))
        ));
        assert!(matches!(
            ExtensionId::new("two.segments"),
            Err(IdError::InvalidExtensionId(_))
        ));
        assert!(matches!(
            ExtensionId::new("UPPER.CASE.SEGMENTS"),
            Err(IdError::InvalidExtensionId(_))
        ));
        assert!(matches!(
            ExtensionId::new("org.shilpo.invalid_underscore"),
            Err(IdError::InvalidExtensionId(_))
        ));
    }

    #[test]
    fn test_valid_contribution_id() {
        let id = ContributionId::new("weather-widget_v1").unwrap();
        assert_eq!(id.as_str(), "weather-widget_v1");
        assert_eq!(id.to_string(), "weather-widget_v1");
    }

    #[test]
    fn test_invalid_contribution_id() {
        assert!(matches!(
            ContributionId::new("-leading-dash"),
            Err(IdError::InvalidContributionId(_))
        ));
        assert!(matches!(
            ContributionId::new("invalid/slash"),
            Err(IdError::InvalidContributionId(_))
        ));
        assert!(matches!(
            ContributionId::new("InvalidCaps"),
            Err(IdError::InvalidContributionId(_))
        ));
    }

    #[test]
    fn test_canonical_id_parsing_and_display() {
        let canonical: CanonicalId = "io.github.alice/world-clock".parse().unwrap();
        assert_eq!(canonical.extension_id.as_str(), "io.github.alice");
        assert_eq!(canonical.contribution_id.as_str(), "world-clock");
        assert_eq!(canonical.to_string(), "io.github.alice/world-clock");

        assert!(matches!(
            "invalid-no-slash".parse::<CanonicalId>(),
            Err(IdError::InvalidCanonicalId(_))
        ));
        assert!(matches!(
            "io.github.alice/too/many/slashes".parse::<CanonicalId>(),
            Err(IdError::InvalidCanonicalId(_))
        ));
    }

    #[test]
    fn test_serde_canonical_id() {
        let canonical: CanonicalId = "io.github.alice/world-clock".parse().unwrap();
        let json = serde_json::to_string(&canonical).unwrap();
        assert_eq!(json, "\"io.github.alice/world-clock\"");

        let deserialized: CanonicalId = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, canonical);
    }

    mod proptests {
        use super::*;
        use proptest::prelude::*;

        fn is_valid_extension_id_oracle(s: &str) -> bool {
            let parts: Vec<&str> = s.split('.').collect();
            if parts.len() < 3 {
                return false;
            }
            parts.iter().all(|seg| {
                if seg.is_empty() {
                    return false;
                }
                let first = seg.as_bytes()[0];
                if !(first.is_ascii_lowercase() || first.is_ascii_digit()) {
                    return false;
                }
                seg.bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
            })
        }

        fn is_valid_contribution_id_oracle(s: &str) -> bool {
            if s.is_empty() {
                return false;
            }
            let first = s.as_bytes()[0];
            if !(first.is_ascii_lowercase() || first.is_ascii_digit()) {
                return false;
            }
            s.bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_')
        }

        fn is_valid_canonical_id_oracle(s: &str) -> bool {
            let parts: Vec<&str> = s.split('/').collect();
            if parts.len() != 2 {
                return false;
            }
            is_valid_extension_id_oracle(parts[0]) && is_valid_contribution_id_oracle(parts[1])
        }

        proptest! {
            #[test]
            fn test_extension_id_round_trip_and_oracle(
                s in "[a-z0-9][a-z0-9-]{0,6}\\.[a-z0-9][a-z0-9-]{0,6}\\.[a-z0-9][a-z0-9-]{0,6}"
            ) {
                prop_assert!(is_valid_extension_id_oracle(&s));
                let id = ExtensionId::new(&s).expect("valid extension id");
                prop_assert_eq!(id.as_str(), s.as_str());
                prop_assert_eq!(id.to_string(), s.as_str());

                let reconstructed = ExtensionId::new(id.to_string()).expect("reconstructed extension id");
                prop_assert_eq!(&id, &reconstructed);

                let json = serde_json::to_string(&id).expect("json serialize");
                let deserialized: ExtensionId = serde_json::from_str(&json).expect("json deserialize");
                prop_assert_eq!(id, deserialized);
            }

            #[test]
            fn test_contribution_id_round_trip_and_oracle(
                s in "[a-z0-9][a-z0-9_-]{0,12}"
            ) {
                prop_assert!(is_valid_contribution_id_oracle(&s));
                let id = ContributionId::new(&s).expect("valid contribution id");
                prop_assert_eq!(id.as_str(), s.as_str());
                prop_assert_eq!(id.to_string(), s.as_str());

                let reconstructed = ContributionId::new(id.to_string()).expect("reconstructed contribution id");
                prop_assert_eq!(&id, &reconstructed);

                let json = serde_json::to_string(&id).expect("json serialize");
                let deserialized: ContributionId = serde_json::from_str(&json).expect("json deserialize");
                prop_assert_eq!(id, deserialized);
            }

            #[test]
            fn test_canonical_id_round_trip_and_oracle(
                ext_s in "[a-z0-9][a-z0-9-]{0,6}\\.[a-z0-9][a-z0-9-]{0,6}\\.[a-z0-9][a-z0-9-]{0,6}",
                contrib_s in "[a-z0-9][a-z0-9_-]{0,12}",
            ) {
                let combined = format!("{ext_s}/{contrib_s}");
                prop_assert!(is_valid_canonical_id_oracle(&combined));

                let ext = ExtensionId::new(&ext_s).unwrap();
                let contrib = ContributionId::new(&contrib_s).unwrap();
                let canonical = CanonicalId::new(ext, contrib);

                prop_assert_eq!(canonical.to_string(), combined.as_str());

                let parsed: CanonicalId = combined.parse().expect("parse canonical id");
                prop_assert_eq!(&canonical, &parsed);

                let json = serde_json::to_string(&canonical).expect("json serialize");
                let deserialized: CanonicalId = serde_json::from_str(&json).expect("json deserialize");
                prop_assert_eq!(canonical, deserialized);
            }

            #[test]
            fn test_arbitrary_string_parsing_no_panic(s in "\\PC*") {
                let ext_res = ExtensionId::new(&s);
                prop_assert_eq!(ext_res.is_ok(), is_valid_extension_id_oracle(&s));

                let contrib_res = ContributionId::new(&s);
                prop_assert_eq!(contrib_res.is_ok(), is_valid_contribution_id_oracle(&s));

                let canonical_res: Result<CanonicalId, _> = s.parse();
                prop_assert_eq!(canonical_res.is_ok(), is_valid_canonical_id_oracle(&s));
            }
        }
    }
}
