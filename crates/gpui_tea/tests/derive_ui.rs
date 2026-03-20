//! Test or Example
#[test]
fn composite_derive_reports_invalid_usage() {
    let cases = trybuild::TestCases::new();
    cases.compile_fail("tests/ui/*.rs");
}
