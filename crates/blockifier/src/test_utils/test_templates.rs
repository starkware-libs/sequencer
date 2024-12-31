use rstest_reuse::template;

#[cfg(not(feature = "cairo_native"))]
#[template]
#[rstest]
fn cairo_version_no_native(
    #[values(CairoVersion::Cairo0, CairoVersion::Cairo1(RunnableCairo1::Casm))]
    cairo_version: CairoVersion,
) {
}

#[cfg(feature = "cairo_native")]
#[template]
#[rstest]
fn cairo_version_with_native(
    #[values(
        CairoVersion::Cairo0,
        CairoVersion::Cairo1(RunnableCairo1::Casm),
        CairoVersion::Cairo1(RunnableCairo1::Native)
    )]
    cairo_version: CairoVersion,
) {
}

#[cfg(not(feature = "cairo_native"))]
#[template]
#[rstest]
fn cairo_version_no_native(
    #[values(CairoVersion::Cairo0, CairoVersion::Cairo1(RunnableCairo1::Casm))]
    cairo_version: CairoVersion,
) {
}
