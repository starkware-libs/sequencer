use crate::db::serialization::{StorageSerde, StorageSerdeError};

const ABSENT_PREFIX: u8 = 0x00;
const PRESENT_PREFIX: u8 = 0x01;

/// Wraps a value with a presence prefix for pre-image tables.
/// `0x00` = key was absent, `0x01` + serialized value = key was present.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PresencePrefixed<T> {
    /// The key did not exist before this block.
    Absent,
    /// The key existed with this value before this block.
    Present(T),
}

impl<T: StorageSerde> StorageSerde for PresencePrefixed<T> {
    fn serialize_into(&self, writer: &mut impl std::io::Write) -> Result<(), StorageSerdeError> {
        match self {
            PresencePrefixed::Absent => Ok(writer.write_all(&[ABSENT_PREFIX])?),
            PresencePrefixed::Present(value) => {
                writer.write_all(&[PRESENT_PREFIX])?;
                value.serialize_into(writer)
            }
        }
    }

    fn deserialize_from(reader: &mut impl std::io::Read) -> Option<Self> {
        let mut prefix = [0u8; 1];
        reader.read_exact(&mut prefix).ok()?;
        match prefix[0] {
            ABSENT_PREFIX => Some(PresencePrefixed::Absent),
            PRESENT_PREFIX => Some(PresencePrefixed::Present(T::deserialize_from(reader)?)),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use starknet_types_core::felt::Felt;

    use super::*;

    #[test]
    fn absent_roundtrip() {
        let value: PresencePrefixed<Felt> = PresencePrefixed::Absent;
        let mut buf = Vec::new();
        value.serialize_into(&mut buf).unwrap();
        assert_eq!(buf, vec![0x00]);
        let deserialized = PresencePrefixed::<Felt>::deserialize_from(&mut buf.as_slice());
        assert_eq!(deserialized, Some(PresencePrefixed::Absent));
    }

    #[test]
    fn present_roundtrip() {
        let felt = Felt::from(42u64);
        let value = PresencePrefixed::Present(felt);
        let mut buf = Vec::new();
        value.serialize_into(&mut buf).unwrap();
        assert_eq!(buf[0], 0x01);
        let deserialized = PresencePrefixed::<Felt>::deserialize_from(&mut buf.as_slice());
        assert_eq!(deserialized, Some(PresencePrefixed::Present(felt)));
    }

    #[test]
    fn invalid_prefix_returns_none() {
        let buf = vec![0x02, 0x00];
        let deserialized = PresencePrefixed::<Felt>::deserialize_from(&mut buf.as_slice());
        assert_eq!(deserialized, None);
    }
}
