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
fn cairo_version(
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
fn two_cairo_versions(
    #[values(CairoVersion::Cairo0, CairoVersion::Cairo1(RunnableCairo1::Casm))]
    cairo_version: CairoVersion,
    #[values(CairoVersion::Cairo0, CairoVersion::Cairo1(RunnableCairo1::Casm))]
    other_cairo_version: CairoVersion,
) {
}

#[cfg(feature = "cairo_native")]
#[template]
#[rstest]
fn two_cairo_versions(
    #[values(
        CairoVersion::Cairo0,
        CairoVersion::Cairo1(RunnableCairo1::Casm),
        CairoVersion::Cairo1(RunnableCairo1::Native)
    )]
    cairo_version: CairoVersion,
    #[values(
        CairoVersion::Cairo0,
        CairoVersion::Cairo1(RunnableCairo1::Casm),
        CairoVersion::Cairo1(RunnableCairo1::Native)
    )]
    other_cairo_version: CairoVersion,
) {
}
