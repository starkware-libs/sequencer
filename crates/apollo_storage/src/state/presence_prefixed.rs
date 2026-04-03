use crate::db::serialization::{StorageSerde, StorageSerdeError};

/// Wraps an `Option<T>` with presence semantics for pre-image tables.
/// `None` = key was absent before this block, `Some(value)` = key existed with this value.
///
/// Serialization delegates to `Option<T>`: `0x00` = absent, `0x01` + value = present.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PresencePrefixed<T>(pub Option<T>);

impl<T> PresencePrefixed<T> {
    pub fn absent() -> Self {
        Self(None)
    }
}

impl<T> From<Option<T>> for PresencePrefixed<T> {
    fn from(option: Option<T>) -> Self {
        Self(option)
    }
}

impl<T: StorageSerde> StorageSerde for PresencePrefixed<T> {
    fn serialize_into(&self, writer: &mut impl std::io::Write) -> Result<(), StorageSerdeError> {
        self.0.serialize_into(writer)
    }

    fn deserialize_from(reader: &mut impl std::io::Read) -> Option<Self> {
        Option::<T>::deserialize_from(reader).map(PresencePrefixed)
    }
}
