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

pub use apollo_sizeof_macros::SizeOf;

#[cfg(test)]
mod tests {
    use std::env;
    use std::ops::Deref;
    use std::rc::Rc;
    use std::sync::Arc;

    use starknet_types_core::felt::Felt;

    use super::SizeOf;

    #[test]
    fn regression_test_size_of() {
        assert_eq!(17_u8.size_bytes(), 1);

        assert_eq!(String::from("Hello").size_bytes(), 29);

        assert_eq!(vec![1_u8, 2_u8, 3_u8].size_bytes(), 27);

        #[derive(SizeOf)]
        struct MyStruct {
            a: u32,
            b: String,
            c: Vec<u8>,
        }
        let strct = MyStruct { a: 42, b: String::from("Hello"), c: vec![1, 2, 3, 4, 5] };
        assert_eq!(strct.size_bytes(), 66);

        #[derive(SizeOf)]
        enum MyEnum {
            VariantA(u32),
            VariantB { x: u64, y: String },
        }
        let my_enum_a = MyEnum::VariantA(42);
        assert_eq!(my_enum_a.size_bytes(), 32);

        let my_enum_b = MyEnum::VariantB { x: 100, y: String::from("World") };
        assert_eq!(my_enum_b.size_bytes(), 37);

        #[derive(SizeOf)]
        enum MyComplicatedEnum {
            VariantA(MyStruct),
            VariantB(Vec<MyEnum>),
        }
        let my_complicated_enum_a = MyComplicatedEnum::VariantA(MyStruct {
            a: 42,
            b: String::from("Hello"),
            c: vec![1, 2, 3],
        });
        assert_eq!(my_complicated_enum_a.size_bytes(), 64);

        let my_complicated_enum_b = MyComplicatedEnum::VariantB(vec![
            MyEnum::VariantA(42),
            MyEnum::VariantB { x: 100, y: String::from("World") },
            MyEnum::VariantB { x: 66, y: String::from("Starknet") },
        ]);
        assert_eq!(my_complicated_enum_b.size_bytes(), 165);
    }

    #[test]
    fn test_size_of_struct() {
        #[derive(SizeOf)]
        struct MyStruct {
            a: u32,
            b: String,
            c: Vec<u8>,
        }
        let my_struct = MyStruct { a: 42, b: String::from("Hello"), c: vec![1, 2, 3, 4, 5] };
        assert_eq!(my_struct.size_bytes(), std::mem::size_of::<MyStruct>() + 5 + 5);
    }

    #[test]
    fn test_size_of_enum() {
        #[derive(SizeOf)]
        enum MyEnum {
            VariantA(u32),
            VariantB { x: u64, y: String },
        }
        let my_enum_a = MyEnum::VariantA(42);
        let my_enum_b = MyEnum::VariantB { x: 100, y: String::from("World!") };
        assert_eq!(my_enum_a.size_bytes(), std::mem::size_of::<MyEnum>());
        assert_eq!(my_enum_b.size_bytes(), std::mem::size_of::<MyEnum>() + 6);
    }

    #[test]
    fn test_size_of_complicated_enum() {
        #[derive(SizeOf)]
        enum MyEnum {
            VariantA(u32),
            VariantB { x: u64, y: String },
        }
        #[derive(SizeOf)]
        struct MyStruct {
            a: u32,
            b: String,
            c: Vec<u8>,
        }
        #[derive(SizeOf)]
        enum MyComplicatedEnum {
            VariantA(MyStruct),
            VariantB { vec: Vec<MyEnum> },
        }
        let my_complicated_enum_a = MyComplicatedEnum::VariantA(MyStruct {
            a: 42,
            b: String::from("Hello"),
            c: vec![1, 2, 3],
        });
        let my_complicated_enum_b = MyComplicatedEnum::VariantB {
            vec: vec![MyEnum::VariantA(42), MyEnum::VariantB { x: 100, y: String::from("World!") }],
        };
        assert_eq!(
            my_complicated_enum_a.size_bytes(),
            std::mem::size_of::<MyComplicatedEnum>() + 5 + 3
        );
        assert_eq!(
            my_complicated_enum_b.size_bytes(),
            std::mem::size_of::<MyComplicatedEnum>() + 2 * std::mem::size_of::<MyEnum>() + 6
        );
    }

    #[test]
    fn test_should_not_compile() {
        // Note: this sets the TRYBUILD=overwrite env variable globally. If used elsewhere in the
        // future, consider changing this or removing this test.
        env::set_var("TRYBUILD", "overwrite");
        let t = trybuild::TestCases::new();
        t.compile_fail("negative_tests/*.rs");
    }

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
}
