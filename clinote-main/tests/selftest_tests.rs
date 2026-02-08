use clinote::selftest;
use clinote::validate::Template;

#[test]
fn selftest_directory_mode() {
    let summary = selftest::run_selftest("tests/fixtures", Template::Soap, false, None).unwrap();
    assert!(summary.total_files > 0);
    assert!(summary.total_notes > 0);
}

#[test]
fn selftest_glob_mode() {
    let summary = selftest::run_selftest("tests/fixtures/*.txt", Template::Soap, false, None).unwrap();
    assert!(summary.total_files > 0);
}

#[test]
fn selftest_strict_flags_errors() {
    let summary = selftest::run_selftest("tests/fixtures/invalid_soap.txt", Template::Soap, true, None).unwrap();
    assert!(summary.total_errors > 0);
}

#[test]
fn selftest_json_output_parses() {
    let summary = selftest::run_selftest("tests/fixtures/soap_messy.txt", Template::Soap, false, None).unwrap();
    let json = serde_json::to_string(&summary).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert!(parsed.get("total_files").is_some());
}
