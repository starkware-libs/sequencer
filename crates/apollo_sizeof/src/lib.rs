use std::ops::Deref;
use std::rc::Rc;
use std::sync::Arc;

use starknet_types_core::felt::Felt;

#[cfg(test)]
#[path = "test.rs"]
mod test;

/// Trait for types that can report their size in bytes.
/// There are two methods:
/// - `dynamic_size`: returns the size of the heap part of the type.
/// - `size_bytes`: returns the total size of the type, including both stack and heap parts.
///
/// This trait is useful for calculating the size of types in memory, especially when dealing with
/// dynamic data structures like `Vec<T>` or `String`.
pub trait SizeOf {
    /// Returns the heap size of the type in bytes.
    ///
    /// `WARNING`: It is `DANGEROUS` to use this function on types that implement `Deref`, since
    /// `Deref coercion` will neglect counting the    stack size taken by the original type
    /// itself. Currently, this trait is only implemented for the    following `Deref` types:
    /// `Box<T>, Rc<T>, Arc<T>`. If your `Deref` type is not on this list, refrain from using
    /// this method.
    fn dynamic_size(&self) -> usize;

    /// Returns the total size of the type in bytes, including both stack and heap parts.
    ///
    /// `WARNING`: It is `DANGEROUS` to use this function on types that implement `Deref`, since
    /// `Deref coercion` will neglect counting the    stack size taken by the original type
    /// itself. Currently, this trait is only implemented for the    following `Deref` types:
    /// `Box<T>, Rc<T>, Arc<T>`. If your `Deref` type is not on this list, refrain from using
    /// this method.
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
        let used = self.iter().map(|x| x.size_bytes()).sum::<usize>();
        let excess = (self.capacity() - self.len()) * std::mem::size_of::<T>();

        used + excess
    }
}

#[macro_export]
macro_rules! default_deref_sizeof {
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

default_deref_sizeof! { Box<T>, Rc<T>, Arc<T> }

pub use apollo_sizeof_macros::SizeOf;
