extern crate starknet_types_core;

use std::ops::Deref;

use starknet_types_core::felt::Felt;

/// Trait for types that can report their size in bytes.
/// There are two methods:
/// - `dynamic_size`: returns the size of the heap part of the type.
/// - `size_bytes`: returns the total size of the type, including both stack and heap parts.
///
/// This trait is useful for calculating the size of types in memory, especially when dealing with
/// dynamic data structures like `Vec<T>` or `String`.
pub trait SizeOf {
    /// Returns the heap size of the type in bytes.
    fn dynamic_size(&self) -> usize;

    /// Returns the total size of the type in bytes, including both stack and heap parts.
    fn size_bytes(&self) -> usize
    where
        Self: Sized,
    {
        std::mem::size_of::<Self>() + self.dynamic_size()
    }
}

#[macro_export]
macro_rules! default_primitive_sizeof {
    ($($type:ty),*) => {
        $(
            impl SizeOf for $type {
                fn dynamic_size(&self) -> usize {
                    0
                }
            }
        )*
    };
}

default_primitive_sizeof! {
    bool, u8, i8, u16, i16, u32, i32, u64, i64, u128, i128,
    f32, f64,
    usize, isize,
    Felt
}

impl SizeOf for String {
    fn dynamic_size(&self) -> usize {
        self.capacity()
    }
}

impl<T: SizeOf> SizeOf for Vec<T> {
    fn dynamic_size(&self) -> usize {
        self.iter().map(|x| x.size_bytes()).sum::<usize>()
    }
}

impl<T: SizeOf> SizeOf for Box<T> {
    fn dynamic_size(&self) -> usize {
        self.deref().size_bytes()
    }
}

#[test]
fn test_felt_size_of() {
    assert_eq!(Felt::ZERO.size_bytes(), 32);
    assert_eq!(Felt::ONE.size_bytes(), 32);
    assert_eq!(Felt::from(1600000000).size_bytes(), 32);
    assert_eq!(Felt::MAX.size_bytes(), 32);
}
