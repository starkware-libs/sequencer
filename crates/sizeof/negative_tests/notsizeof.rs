use sizeof::SizeOf;
struct NotSizeOf {
    a: u32,
    b: String,
}
#[derive(SizeOf)]
struct ShouldNotCompile {
    a: u32,
    b: NotSizeOf,
}

fn main() {
    // This test is expected to fail because `NotSizeOf` does not implement `SizeOf`.
    // The macro should not compile.

    let _ = ShouldNotCompile { a: 42, b: NotSizeOf { a: 1, b: String::from("test") } };
    println!(
        "This is a negative test for the SizeOf macro. It should not compile if the macro is working correctly. You can find the expected error in notsizeof.stderr"
    );
}