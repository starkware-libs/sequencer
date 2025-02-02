#[cfg(test)]
use blockifier_test_utils::cairo_versions::RunnableCairo1;
use rstest::rstest;
use rstest_reuse::{apply, template};

#[cfg(test)]
use crate::test_utils::CairoVersion;
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
    cairo_version1: CairoVersion,
    #[values(CairoVersion::Cairo0, CairoVersion::Cairo1(RunnableCairo1::Casm))]
    cairo_version2: CairoVersion,
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
    cairo_version1: CairoVersion,
    #[values(
        CairoVersion::Cairo0,
        CairoVersion::Cairo1(RunnableCairo1::Casm),
        CairoVersion::Cairo1(RunnableCairo1::Native)
    )]
    cairo_version2: CairoVersion,
) {
}

// When creating a function using a template, every function name is slightly different to avoid
// having multiple functions with the same name in the same scope. This means that the fact that we
// do not use the template in the file it's in makes the compiler think it is an unused macro.
// To avoid this we created a dummy test that uses the template.

#[apply(cairo_version)]
fn test_cairo_version(cairo_version: CairoVersion) {
    println!("test {:?}", cairo_version);
}

#[apply(two_cairo_versions)]
fn test_two_cairo_version(cairo_version1: CairoVersion, cairo_version2: CairoVersion) {
    println!("test {:?} {:?}", cairo_version1, cairo_version2);
}
