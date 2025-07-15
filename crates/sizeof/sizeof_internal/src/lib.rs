use std::cell::RefCell;
use std::ops::Deref;
use std::rc::Rc;
use std::sync::{Arc, Mutex, RwLock};

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

#[macro_export]
macro_rules! default_pointer_sizeof {
    ($($name:ident < $($generic_params:ident $(: $bounds:path)*),* >),*) => {
        $(
             impl < $($generic_params : $crate::SizeOf $(+ $bounds)* ),* > $crate::SizeOf for $name < $($generic_params),* >  {
                fn dynamic_size(&self) -> usize {
                    self.deref().size_bytes()
                }
            }
        )*
    };
}

default_pointer_sizeof! { Box<T>, Rc<T>, Arc<T>, RefCell<T>, Mutex<T>, RwLock<T> }

#[test]
fn test_size_of() {
    assert_eq!(17_u8.size_bytes(), 1);
    assert_eq!(
        String::from("Hello").size_bytes(),
        std::mem::size_of::<String>() + String::from("Hello").capacity()
    );
    assert_eq!(
        vec![1_u8, 2_u8, 3_u8].size_bytes(),
        std::mem::size_of::<Vec<u8>>() + std::mem::size_of::<u8>() * 3
    );
}

#[test]
fn test_felt_size_of() {
    assert_eq!(Felt::ZERO.size_bytes(), 32);
    assert_eq!(Felt::ONE.size_bytes(), 32);
    assert_eq!(Felt::from(1600000000).size_bytes(), 32);
    assert_eq!(Felt::MAX.size_bytes(), 32);
}


