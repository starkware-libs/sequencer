extern crate starknet_types_core;

use starknet_types_core::felt::Felt;

pub trait SizeOf {
    /// Returns the size (stack+heap) of the type in bytes.
    fn size_bytes(&self) -> usize;
}

#[macro_export]
macro_rules! default_primitive_sizeof {
    ($($type:ty),*) => {
        $(
            impl SizeOf for $type {
                fn size_bytes(&self) -> usize {
                    std::mem::size_of::<$type>()
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
    fn size_bytes(&self) -> usize {
        std::mem::size_of::<Self>() + self.capacity()
    }
}

impl<T: SizeOf> SizeOf for Vec<T> {
    fn size_bytes(&self) -> usize {
        std::mem::size_of::<Self>() + self.iter().map(|x| x.size_bytes()).sum::<usize>()
    }
}

#[test]
fn felt_size_of() {
    assert_eq!(Felt::ZERO.size_bytes(), 32);
    assert_eq!(Felt::ONE.size_bytes(), 32);
    assert_eq!(Felt::from(1600000000).size_bytes(), 32);
    assert_eq!(Felt::MAX.size_bytes(), 32);
}
