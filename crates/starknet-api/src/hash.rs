#[cfg(test)]
#[path = "hash_test.rs"]
mod hash_test;

use std::fmt::{Debug, Display};
use std::io::Error;

use serde::{Deserialize, Serialize};
use starknet_crypto::FieldElement;
use starknet_types_core::felt::Felt;
use starknet_types_core::hash::{Pedersen, Poseidon, StarkHash as StarknetTypesStarkHash};

use crate::serde_utils::{bytes_from_hex_str, hex_str_from_bytes, BytesAsHex, PrefixedBytesAsHex};
use crate::{impl_from_through_intermediate, StarknetApiError};

/// Genesis state hash.
pub const GENESIS_HASH: &str = "0x0";

// Felt encoding constants.
const CHOOSER_FULL: u8 = 15;
const CHOOSER_HALF: u8 = 14;

/// An alias for [`StarkFelt`].
/// The output of the [Pedersen hash](https://docs.starknet.io/documentation/architecture_and_concepts/Hashing/hash-functions/#pedersen_hash).
pub type StarkHash = StarkFelt;

/// Computes Pedersen hash using STARK curve on two elements, as defined
/// in <https://docs.starknet.io/documentation/architecture_and_concepts/Hashing/hash-functions/#pedersen_hash.>
pub fn pedersen_hash(felt0: &StarkFelt, felt1: &StarkFelt) -> StarkHash {
    Pedersen::hash(&Felt::from(felt0), &Felt::from(felt1)).into()
}

/// Computes Pedersen hash using STARK curve on an array of elements, as defined
/// in <https://docs.starknet.io/documentation/architecture_and_concepts/Hashing/hash-functions/#array_hashing.>
pub fn pedersen_hash_array(felts: &[StarkFelt]) -> StarkHash {
    let current_hash = felts.iter().fold(Felt::from(0_u8), |current_hash, stark_felt| {
        Pedersen::hash(&current_hash, &Felt::from(stark_felt))
    });
    let data_len = Felt::from(u128::try_from(felts.len()).expect("Got 2^128 felts or more."));
    Pedersen::hash(&current_hash, &data_len).into()
}

/// A Poseidon hash.
#[derive(
    Copy, Clone, Eq, PartialEq, Default, Hash, Deserialize, Serialize, PartialOrd, Ord, Debug,
)]
pub struct PoseidonHash(pub StarkFelt);

impl Display for PoseidonHash {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Display::fmt(&self.0, f)
    }
}

/// Computes Poseidon hash.
pub fn poseidon_hash_array(stark_felts: &[StarkFelt]) -> PoseidonHash {
    // TODO(yair): Avoid allocating the vector of Felts.
    let as_felts = stark_felts.iter().map(Felt::from).collect::<Vec<_>>();
    PoseidonHash(Poseidon::hash_array(as_felts.as_slice()).into())
}

// TODO: Move to a different crate.
/// The StarkNet [field element](https://docs.starknet.io/documentation/architecture_and_concepts/Hashing/hash-functions/#domain_and_range).
#[derive(Copy, Clone, Eq, PartialEq, Default, Hash, Deserialize, Serialize, PartialOrd, Ord)]
#[serde(try_from = "PrefixedBytesAsHex<32_usize>", into = "PrefixedBytesAsHex<32_usize>")]
pub struct StarkFelt([u8; 32]);

impl StarkFelt {
    /// Returns a new [`StarkFelt`].
    pub fn new(bytes: [u8; 32]) -> Result<StarkFelt, StarknetApiError> {
        // msb nibble must be 0. This is not a tight bound.
        if bytes[0] < 0x10 {
            return Ok(Self(bytes));
        }
        Err(StarknetApiError::OutOfRange { string: hex_str_from_bytes::<32, true>(bytes) })
    }

    /// Returns a new *unchecked* [`StarkFelt`]
    ///
    /// # Safety
    ///
    /// To avoid undefined behavior, refer to [`StarkFelt`] struct's docstring
    /// for the required constraints on the `bytes` argument, or use [`StarkFelt::new`] instead of
    /// this method.
    ///
    /// # Usage
    ///
    /// Most of the time you should use `new` instead, but it comes in handy for a few cases:
    /// - creating instances of `StarkFelt` at compile time
    /// - implementing `From<T> for StarkFelt` for types that have a smaller binary representation
    ///   than `StarkFelt`
    pub const fn new_unchecked(bytes: [u8; 32]) -> StarkFelt {
        Self(bytes)
    }

    /// [StarkFelt] constant that's equal to 0.
    pub const ZERO: Self = { Self::from_u128(0_u128) };

    /// [StarkFelt] constant that's equal to 1.
    pub const ONE: Self = { Self::from_u128(1_u128) };

    /// [StarkFelt] constant that's equal to 2.
    pub const TWO: Self = { Self::from_u128(2_u128) };

    /// [StarkFelt] constant that's equal to 3.
    pub const THREE: Self = { Self::from_u128(3_u128) };

    pub const fn from_u128(val: u128) -> Self {
        let mut bytes = [0u8; 32];
        let val_bytes = val.to_be_bytes();
        let mut index = 16;
        while index < 32 {
            bytes[index] = val_bytes[index - 16];
            index += 1;
        }
        Self(bytes)
    }

    /// Storage efficient serialization for field elements.
    pub fn serialize(&self, res: &mut impl std::io::Write) -> Result<(), Error> {
        // We use the fact that bytes[0] < 0x10 and encode the size of the felt in the 4 most
        // significant bits of the serialization, which we call `chooser`. We assume that 128 bit
        // felts are prevalent (because of how uint256 is encoded in felts).

        // The first i for which nibbles 2i+1, 2i+2 are nonzero. Note that the first nibble is
        // always 0.
        let mut first_index = 31;
        for i in 0..32 {
            let value = self.0[i];
            if value == 0 {
                continue;
            } else if value < 16 {
                // Can encode the chooser and the value on a single byte.
                first_index = i;
            } else {
                // The chooser is encoded with the first nibble of the value.
                first_index = i - 1;
            }
            break;
        }
        let chooser = if first_index < 15 {
            // For 34 up to 63 nibble felts: chooser == 15, serialize using 32 bytes.
            first_index = 0;
            CHOOSER_FULL
        } else if first_index < 18 {
            // For 28 up to 33 nibble felts: chooser == 14, serialize using 17 bytes.
            first_index = 15;
            CHOOSER_HALF
        } else {
            // For up to 27 nibble felts: serialize the lower 1 + (chooser * 2) nibbles of the felt
            // using chooser + 1 bytes.
            (31 - first_index) as u8
        };
        res.write_all(&[(chooser << 4) | self.0[first_index]])?;
        res.write_all(&self.0[first_index + 1..])?;
        Ok(())
    }

    /// Storage efficient deserialization for field elements.
    pub fn deserialize(bytes: &mut impl std::io::Read) -> Option<Self> {
        let mut res = [0u8; 32];

        bytes.read_exact(&mut res[..1]).ok()?;
        let first = res[0];
        let chooser: u8 = first >> 4;
        let first = first & 0x0f;

        let first_index = if chooser == CHOOSER_FULL {
            0
        } else if chooser == CHOOSER_HALF {
            15
        } else {
            (31 - chooser) as usize
        };
        res[0] = 0;
        res[first_index] = first;
        bytes.read_exact(&mut res[first_index + 1..]).ok()?;
        Some(Self(res))
    }

    pub fn bytes(&self) -> &[u8; 32] {
        &self.0
    }

    fn str_format(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = format!("0x{}", hex::encode(self.0));
        f.debug_tuple("StarkFelt").field(&s).finish()
    }
}

impl TryFrom<PrefixedBytesAsHex<32_usize>> for StarkFelt {
    type Error = StarknetApiError;
    fn try_from(val: PrefixedBytesAsHex<32_usize>) -> Result<Self, Self::Error> {
        StarkFelt::new(val.0)
    }
}

impl TryFrom<&str> for StarkFelt {
    type Error = StarknetApiError;
    fn try_from(val: &str) -> Result<Self, Self::Error> {
        let val = val.trim_start_matches("0x");
        let bytes = bytes_from_hex_str::<32, false>(val)?;
        Self::new(bytes)
    }
}

impl From<u128> for StarkFelt {
    fn from(val: u128) -> Self {
        Self::from_u128(val)
    }
}

impl_from_through_intermediate!(u128, StarkFelt, u8, u16, u32, u64);

impl From<Felt> for StarkFelt {
    fn from(felt: Felt) -> Self {
        // Should not fail.
        Self::new(felt.to_bytes_be()).expect("Convert Felt to StarkFelt.")
    }
}

impl From<usize> for StarkFelt {
    fn from(val: usize) -> Self {
        Felt::from(val).into()
    }
}

impl From<&StarkFelt> for Felt {
    fn from(felt: &StarkFelt) -> Self {
        Self::from_bytes_be(&felt.0)
    }
}

impl From<FieldElement> for StarkFelt {
    fn from(fe: FieldElement) -> Self {
        // Should not fail.
        Self::new(fe.to_bytes_be()).expect("Convert FieldElement to StarkFelt.")
    }
}

impl From<StarkFelt> for FieldElement {
    fn from(felt: StarkFelt) -> Self {
        // Should not fail.
        Self::from_bytes_be(&felt.0).expect("Convert StarkFelf to FieldElement.")
    }
}

impl From<StarkFelt> for PrefixedBytesAsHex<32_usize> {
    fn from(felt: StarkFelt) -> Self {
        BytesAsHex(felt.0)
    }
}

// TODO(Arni, 25/6/2023): Remove impl TryFrom<StarkFelt> for usize. Leave only one conversion from
//  StarkFelt to integer type.
impl TryFrom<StarkFelt> for usize {
    type Error = StarknetApiError;
    fn try_from(felt: StarkFelt) -> Result<Self, Self::Error> {
        const COMPLIMENT_OF_USIZE: usize =
            std::mem::size_of::<StarkFelt>() - std::mem::size_of::<usize>();

        let (rest, usize_bytes) = felt.bytes().split_at(COMPLIMENT_OF_USIZE);
        if rest != [0u8; COMPLIMENT_OF_USIZE] {
            return Err(StarknetApiError::OutOfRange { string: felt.to_string() });
        }

        Ok(usize::from_be_bytes(
            usize_bytes.try_into().expect("usize_bytes should be of size usize."),
        ))
    }
}

// TODO(Arni, 1/1/2024): This is a Hack. Remove this and implement arethmetics for StarkFelt.
impl TryFrom<StarkFelt> for u64 {
    type Error = StarknetApiError;
    fn try_from(felt: StarkFelt) -> Result<Self, Self::Error> {
        const COMPLIMENT_OF_U64: usize = 24; // 32 - 8
        let (rest, u64_bytes) = felt.bytes().split_at(COMPLIMENT_OF_U64);
        if rest != [0u8; COMPLIMENT_OF_U64] {
            return Err(StarknetApiError::OutOfRange { string: felt.to_string() });
        }

        let bytes: [u8; 8] = u64_bytes.try_into().unwrap();
        Ok(u64::from_be_bytes(bytes))
    }
}

impl Debug for StarkFelt {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.str_format(f)
    }
}

impl Display for StarkFelt {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "0x{}", hex::encode(self.0))
    }
}

/// A utility macro to create a [`StarkFelt`] from a hex string representation.
#[cfg(any(feature = "testing", test))]
#[macro_export]
macro_rules! stark_felt {
    ($s:expr) => {
        StarkFelt::try_from($s).unwrap()
    };
}
