//! Ergonomic helpers and conversions for WIT [`DataValue`].

use crate::bindings::shilpo::extension::types::{DataValue, SecretRef};

/// Conversion trait into a canonical [`DataValue`].
pub trait IntoDataValue {
    /// Converts this value into a [`DataValue`].
    fn into_data_value(self) -> DataValue;
}

impl IntoDataValue for DataValue {
    fn into_data_value(self) -> DataValue {
        self
    }
}

impl IntoDataValue for bool {
    fn into_data_value(self) -> DataValue {
        DataValue::BoolValue(self)
    }
}

impl IntoDataValue for i64 {
    fn into_data_value(self) -> DataValue {
        DataValue::IntValue(self)
    }
}

impl IntoDataValue for i32 {
    fn into_data_value(self) -> DataValue {
        DataValue::IntValue(self as i64)
    }
}

impl IntoDataValue for u32 {
    fn into_data_value(self) -> DataValue {
        DataValue::IntValue(self as i64)
    }
}

impl IntoDataValue for f64 {
    fn into_data_value(self) -> DataValue {
        DataValue::FloatValue(self)
    }
}

impl IntoDataValue for f32 {
    fn into_data_value(self) -> DataValue {
        DataValue::FloatValue(self as f64)
    }
}

impl IntoDataValue for String {
    fn into_data_value(self) -> DataValue {
        DataValue::TextValue(self)
    }
}

impl IntoDataValue for &str {
    fn into_data_value(self) -> DataValue {
        DataValue::TextValue(self.to_string())
    }
}

impl IntoDataValue for Vec<u8> {
    fn into_data_value(self) -> DataValue {
        DataValue::BytesValue(self)
    }
}

impl IntoDataValue for &[u8] {
    fn into_data_value(self) -> DataValue {
        DataValue::BytesValue(self.to_vec())
    }
}

/// Extension trait providing constructors and extractors for [`DataValue`].
pub trait DataValueExt {
    /// Creates a `DataValue::None`.
    fn none() -> DataValue {
        DataValue::None
    }

    /// Creates a boolean `DataValue`.
    fn from_bool(val: bool) -> DataValue {
        DataValue::BoolValue(val)
    }

    /// Creates an integer `DataValue`.
    fn from_int(val: i64) -> DataValue {
        DataValue::IntValue(val)
    }

    /// Creates a float `DataValue`.
    fn from_float(val: f64) -> DataValue {
        DataValue::FloatValue(val)
    }

    /// Creates a text `DataValue`.
    fn from_text(val: impl Into<String>) -> DataValue {
        DataValue::TextValue(val.into())
    }

    /// Creates a bytes `DataValue`.
    fn from_bytes(val: impl Into<Vec<u8>>) -> DataValue {
        DataValue::BytesValue(val.into())
    }

    /// Creates a secret reference `DataValue`.
    fn from_secret_ref(handle: impl Into<String>) -> DataValue {
        DataValue::SecretRef(SecretRef {
            handle: handle.into(),
        })
    }

    /// Extracts boolean value if this is a [`DataValue::BoolValue`].
    fn as_bool(&self) -> Option<bool>;

    /// Extracts integer value if this is an [`DataValue::IntValue`].
    fn as_int(&self) -> Option<i64>;

    /// Extracts float value if this is a [`DataValue::FloatValue`].
    fn as_float(&self) -> Option<f64>;

    /// Extracts string reference if this is a [`DataValue::TextValue`].
    fn as_str(&self) -> Option<&str>;

    /// Extracts byte slice if this is a [`DataValue::BytesValue`].
    fn as_bytes(&self) -> Option<&[u8]>;

    /// Extracts secret handle if this is a [`DataValue::SecretRef`].
    fn as_secret_handle(&self) -> Option<&str>;

    /// Returns `true` if this value is [`DataValue::None`].
    fn is_none(&self) -> bool;
}

impl DataValueExt for DataValue {
    fn as_bool(&self) -> Option<bool> {
        match self {
            Self::BoolValue(v) => Some(*v),
            _ => None,
        }
    }

    fn as_int(&self) -> Option<i64> {
        match self {
            Self::IntValue(v) => Some(*v),
            _ => None,
        }
    }

    fn as_float(&self) -> Option<f64> {
        match self {
            Self::FloatValue(v) => Some(*v),
            _ => None,
        }
    }

    fn as_str(&self) -> Option<&str> {
        match self {
            Self::TextValue(v) => Some(v.as_str()),
            _ => None,
        }
    }

    fn as_bytes(&self) -> Option<&[u8]> {
        match self {
            Self::BytesValue(v) => Some(v.as_slice()),
            _ => None,
        }
    }

    fn as_secret_handle(&self) -> Option<&str> {
        match self {
            Self::SecretRef(s) => Some(s.handle.as_str()),
            _ => None,
        }
    }

    fn is_none(&self) -> bool {
        matches!(self, Self::None)
    }
}
