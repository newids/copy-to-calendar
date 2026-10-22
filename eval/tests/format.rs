use std::path::Path;

use chrono::NaiveDate;
use eval::{LocalTime, load_set, parse_set};

fn fixture(name: &str) -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(name)
}

#[test]
fn prd_examples_fixture_is_valid() {
    let cases = load_set(&fixture("fixtures/prd-examples.jsonl")).unwrap();
    assert_eq!(cases.len(), 3);
    let first = &cases[0].expected.events[0];
    assert_eq!(first.start.to_string(), "2026-10-29T19:00");
    assert_eq!(first.location.as_deref(), Some("강남역 2번 출구"));
    assert_eq!(
        cases[0].reference_date,
        NaiveDate::from_ymd_opt(2026, 10, 19).unwrap()
    );
}

#[test]
fn shipped_set_is_valid() {
    load_set(&fixture("ko-500.jsonl")).unwrap();
}

#[test]
fn all_day_date_parses() {
    let cases = parse_set(
        r#"{"input": "x", "expected": {"events": [{"start": "2026-10-24", "all_day": true}]}, "reference_date": "2026-10-19"}"#,
    )
    .unwrap();
    assert_eq!(
        cases[0].expected.events[0].start,
        LocalTime::Date(NaiveDate::from_ymd_opt(2026, 10, 24).unwrap())
    );
}

#[test]
fn candidate_group_parses() {
    let cases = parse_set(
        r#"{"input": "이번 주 금요일 or 다음 주 화요일 중 편한 시간에", "expected": {"events": [{"start": "2026-10-23", "all_day": true, "candidate_group": 1}, {"start": "2026-10-27", "all_day": true, "candidate_group": 1}]}, "reference_date": "2026-10-19"}"#,
    )
    .unwrap();
    let groups: Vec<_> = cases[0]
        .expected
        .events
        .iter()
        .map(|e| e.candidate_group)
        .collect();
    assert_eq!(groups, [Some(1), Some(1)]);
}

#[test]
fn errors_report_line_numbers() {
    let content = concat!(
        r#"{"input": "x", "expected": {"no_event_found": true}, "reference_date": "2026-10-19"}"#,
        "\n\n",
        r#"{"input": "x", "expected": {}, "reference_date": "2026-10-19", "typo": 1}"#,
        "\n",
        r#"{"input": "x", "expected": {"events": [{"start": "10/24 10시"}]}, "reference_date": "2026-10-19"}"#,
    );
    let errors = parse_set(content).unwrap_err();
    let lines: Vec<usize> = errors.iter().map(|e| e.line).collect();
    assert_eq!(lines, [3, 4]);
}
