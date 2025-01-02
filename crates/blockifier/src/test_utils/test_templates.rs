use rstest_reuse::template;

#[cfg(not(feature = "cairo_native"))]
#[template]
#[rstest]
fn cairo_version(
    #[values(CairoVersion::Cairo0, CairoVersion::Cairo1(RunnableCairo1::Casm))]
    cairo_version: CairoVersion,
) {
}

#[cfg(feature = "cairo_native")]
#[template]
#[rstest]
#[allow(unused_macros)]
fn cairo_version(
    #[values(
        CairoVersion::Cairo0,
        CairoVersion::Cairo1(RunnableCairo1::Casm),
        CairoVersion::Cairo1(RunnableCairo1::Native)
    )]
    cairo_version: CairoVersion,
) {
}