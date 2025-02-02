#[derive(Clone, Hash, PartialEq, Eq, Copy, Debug)]
pub enum RunnableCairo1 {
    Casm,
    #[cfg(feature = "cairo_native")]
    Native,
}

impl Default for RunnableCairo1 {
    fn default() -> Self {
        Self::Casm
    }
}
