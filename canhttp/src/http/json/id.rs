use serde::{Deserialize, Serialize};
use std::fmt::{Display, Formatter};

/// An identifier established by the Client that MUST contain a String, Number, or NULL value if included.
/// 
/// If it is not included it is assumed to be a notification.
/// The value SHOULD normally not be Null.
#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(untagged)]
pub enum Id {
    /// Numeric ID.
    Number(u64),

    /// String ID
    String(String),

    /// Null ID.
    ///
    /// The use of `Null` as a value for the id member in a Request object is discouraged,
    /// because this specification uses a value of Null for Responses with an unknown id.
    /// Also, because JSON-RPC 1.0 uses an id value of Null for Notifications this could cause confusion in handling.
    Null,
}

impl Id {
    /// Zero numeric ID.
    pub const ZERO: Id = Id::Number(0);
}

impl<T: Into<u64>> From<T> for Id {
    fn from(value: T) -> Self {
        Id::Number(value.into())
    }
}

impl Display for Id {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Id::Number(id) => Display::fmt(id, f),
            Id::String(id) => Display::fmt(id, f),
            Id::Null => f.write_str("null"),
        }
    }
}
