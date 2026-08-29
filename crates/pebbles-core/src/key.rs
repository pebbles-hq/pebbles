//! Widget [`Key`]s. During reconciliation two widgets at the same position are
//! considered "the same" (and their element/state reused) only if they have the
//! same concrete type *and* the same key. Keys let you preserve state across
//! reorders; without one, position identity is used.

/// An optional identity for a widget within its parent.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub enum Key {
    /// A string-valued key.
    Value(String),
    /// An integer-valued key.
    Int(u64),
}

impl From<&str> for Key {
    fn from(s: &str) -> Self {
        Key::Value(s.to_owned())
    }
}
impl From<String> for Key {
    fn from(s: String) -> Self {
        Key::Value(s)
    }
}
impl From<u64> for Key {
    fn from(i: u64) -> Self {
        Key::Int(i)
    }
}
