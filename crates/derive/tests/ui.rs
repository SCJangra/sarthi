//! Invalid enums must be rejected with a clear error.

#[test]
fn ui() {
	trybuild::TestCases::new().compile_fail("tests/ui/*.rs");
}
