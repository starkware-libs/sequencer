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
    let my_complicated_enum_a =
        MyComplicatedEnum::VariantA(MyStruct { a: 42, b: String::from("Hello"), c: vec![1, 2, 3] });
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
    let my_complicated_enum_a =
        MyComplicatedEnum::VariantA(MyStruct { a: 42, b: String::from("Hello"), c: vec![1, 2, 3] });
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
