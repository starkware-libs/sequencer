//! Timed test macros for the Apollo sequencer.
//!
//! This crate provides procedural macros for creating tests that fail if they exceed a time limit.
//!
//! # Usage
//!
//! ```rust,ignore
//! use apollo_timed_tests::{timed_test, timed_tokio_test, timed_rstest, timed_rstest_tokio};
//!
//! #[timed_test]
//! fn my_test() {
//!     // test code
//! }
//!
//! #[timed_tokio_test]
//! async fn my_async_test() {
//!     // test code
//! }
//! ```

// Re-export rstest so users don't need to import it explicitly
// Note: Proc macros like rstest, case, and fixture must be imported from the crate:
// `use apollo_timed_tests::rstest;` then use `#[rstest::rstest]`, `#[rstest::case]`, etc.
// Or import directly: `use apollo_timed_tests::rstest::{rstest, case, fixture};`
pub use apollo_timed_tests_macros::{
    timed_rstest,
    timed_rstest_tokio,
    timed_test,
    timed_tokio_test,
};
pub use rstest;
