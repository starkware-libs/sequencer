use std::ops::Deref;
use std::rc::Rc;
use std::sync::Arc;

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

#[test]
fn test_vec_string_sizeof() {
    let mut vec_string = Vec::with_capacity(17);
    vec_string.push(String::from("Hello"));
    vec_string.push(String::from("World!"));
    let size = vec_string.size_bytes();
    assert_eq!(
        size,
        vec_string[0].size_bytes()
            + vec_string[1].size_bytes()
            + (vec_string.capacity() - vec_string.len()) * std::mem::size_of::<String>()
            + std::mem::size_of::<Vec<String>>()
    );
    assert_eq!(vec_string[0].size_bytes(), std::mem::size_of::<String>() + 5);
    assert_eq!(vec_string[1].size_bytes(), std::mem::size_of::<String>() + 6);
}

#[test]
fn test_arc_vec_string_sizeof() {
    let mut vec_string = Vec::with_capacity(35);
    vec_string.push(String::from("Starknet"));
    vec_string.push(String::from("Cairo"));

    let vec_string = Arc::new(vec_string);

    let size = vec_string.size_bytes();
    assert_eq!(
        size,
        vec_string[0].size_bytes()
            + vec_string[1].size_bytes()
            + (vec_string.capacity() - vec_string.len()) * std::mem::size_of::<String>()
            + std::mem::size_of::<Vec<String>>()
            + std::mem::size_of::<Arc<Vec<String>>>()
    );
    assert_eq!(
        vec_string.deref().size_bytes(),
        vec_string[0].size_bytes()
            + vec_string[1].size_bytes()
            + (vec_string.capacity() - vec_string.len()) * std::mem::size_of::<String>()
            + std::mem::size_of::<Vec<String>>()
    );
    assert_eq!(vec_string[0].size_bytes(), std::mem::size_of::<String>() + 8);
    assert_eq!(vec_string[1].size_bytes(), std::mem::size_of::<String>() + 5);
}

#[test]
fn test_rc_vec_string_sizeof() {
    let mut vec_string = Vec::with_capacity(25);
    vec_string.push(String::from("Pip"));
    vec_string.push(String::from("Install"));

    let vec_string = Rc::new(vec_string);

    let size = vec_string.size_bytes();
    assert_eq!(
        size,
        vec_string[0].size_bytes()
            + vec_string[1].size_bytes()
            + (vec_string.capacity() - vec_string.len()) * std::mem::size_of::<String>()
            + std::mem::size_of::<Vec<String>>()
            + std::mem::size_of::<Rc<Vec<String>>>()
    );
    assert_eq!(
        vec_string.deref().size_bytes(),
        vec_string[0].size_bytes()
            + vec_string[1].size_bytes()
            + (vec_string.capacity() - vec_string.len()) * std::mem::size_of::<String>()
            + std::mem::size_of::<Vec<String>>()
    );
    assert_eq!(vec_string[0].size_bytes(), std::mem::size_of::<String>() + 3);
    assert_eq!(vec_string[1].size_bytes(), std::mem::size_of::<String>() + 7);
}

#[test]
fn test_box_vec_string_sizeof() {
    let mut vec_string = Vec::with_capacity(20);
    vec_string.push(String::from("Rust"));
    vec_string.push(String::from("Programming"));
    let vec_string = Box::new(vec_string);
    let size = vec_string.size_bytes();
    assert_eq!(
        size,
        vec_string[0].size_bytes()
            + vec_string[1].size_bytes()
            + (vec_string.capacity() - vec_string.len()) * std::mem::size_of::<String>()
            + std::mem::size_of::<Vec<String>>()
            + std::mem::size_of::<Box<Vec<String>>>()
    );
    assert_eq!(
        vec_string.deref().size_bytes(),
        vec_string[0].size_bytes()
            + vec_string[1].size_bytes()
            + (vec_string.capacity() - vec_string.len()) * std::mem::size_of::<String>()
            + std::mem::size_of::<Vec<String>>()
    );
    assert_eq!(vec_string[0].size_bytes(), std::mem::size_of::<String>() + 4);
    assert_eq!(vec_string[1].size_bytes(), std::mem::size_of::<String>() + 11);
}
