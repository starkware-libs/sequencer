#[cfg(feature = "cairo_native")]
mod cairo_native;

fn main() {
    #[cfg(not(feature = "cairo_native"))]
    {
        eprintln!(
            "The `starknet-native-compile` binary was compiled without the 'cairo_native' feature."
        );
        std::process::exit(1);
    }
    #[cfg(feature = "cairo_native")]
    cairo_native::main();
}
