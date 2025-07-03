pub use sizeof_internal::SizeOf;
pub use sizeof_macro::SizeOf;
#[cfg(test)]
mod tests {
    use starknet_types_core::felt::Felt;

    use super::SizeOf;

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
        let my_enum_b = MyEnum::VariantB { x: 100, y: String::from("World") };
        assert_eq!(my_enum_a.size_bytes(), std::mem::size_of::<MyEnum>());
        assert_eq!(my_enum_b.size_bytes(), std::mem::size_of::<MyEnum>() + 5);
    }

    #[test]
    fn test_should_not_compile() {
        let t = trybuild::TestCases::new();
        t.compile_fail("negative_tests/*.rs");
    }
}
