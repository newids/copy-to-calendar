use c2c_core::holidays::HolidayTable;
use c2c_core::schema::ExtractionResult;
use eval::parse_set;
use eval::score::score_case;
use serde_json::{Value, json};

fn score(case: Value, output: Value) -> Result<(), String> {
    let case = &parse_set(&case.to_string()).unwrap()[0];
    let output: ExtractionResult = serde_json::from_value(output).unwrap();
    score_case(case, &output, &HolidayTable::default())
}

fn case(expected: Value) -> Value {
    json!({"input": "x", "expected": expected, "reference_date": "2026-10-19"})
}

fn rel(value: Value) -> Value {
    json!({"relative": value, "confidence": 0.9})
}

#[test]
fn timed_event() {
    let expected =
        case(json!({"events": [{"start": "2026-10-20T12:00", "end": "2026-10-20T13:00"}]}));
    let lunch = json!({"events": [{"title": null, "start": rel(json!({"day_offset": 1, "time_word": "점심"}))}]});
    assert_eq!(score(expected.clone(), lunch), Ok(()));

    let dinner = json!({"events": [{"title": null, "start": rel(json!({"day_offset": 1, "time_word": "저녁"}))}]});
    let err = score(expected, dinner).unwrap_err();
    assert!(err.contains("시작 2026-10-20T19:00"), "{err}");
}

#[test]
fn end_is_checked_only_when_expected() {
    let expected = case(json!({"events": [{"start": "2026-10-20T12:00"}]}));
    let output = json!({"events": [{"title": null, "start": rel(json!({"day_offset": 1, "hour": 12})),
                                     "end": rel(json!({"hour": 15}))}]});
    assert_eq!(score(expected, output), Ok(()));
}

#[test]
fn all_day_and_counts() {
    let expected = case(json!({"events": [{"start": "2026-10-20", "all_day": true}]}));
    let ok = json!({"events": [{"title": null, "start": rel(json!({"day_offset": 1}))}]});
    assert_eq!(score(expected.clone(), ok), Ok(()));

    let timed =
        json!({"events": [{"title": null, "start": rel(json!({"day_offset": 1, "hour": 15}))}]});
    assert!(score(expected.clone(), timed).is_err());

    let none = json!({"no_event_found": true});
    assert!(score(expected, none).unwrap_err().contains("1건이어야"));
}

#[test]
fn no_event_found() {
    let expected = case(json!({"no_event_found": true}));
    assert_eq!(
        score(expected.clone(), json!({"no_event_found": true})),
        Ok(())
    );
    let some = json!({"events": [{"title": null, "start": rel(json!({"day_offset": 1}))}]});
    assert!(score(expected, some).is_err());
}

#[test]
fn candidate_groups_compare_by_membership() {
    let expected = case(json!({"events": [
        {"start": "2026-10-23", "all_day": true, "candidate_group": 1},
        {"start": "2026-10-27", "all_day": true, "candidate_group": 1}
    ]}));
    let grouped = |a: Value, b: Value| {
        json!({"events": [
            {"title": null, "start": rel(json!({"week_offset": 0, "weekday": 5})), "candidate_group": a},
            {"title": null, "start": rel(json!({"week_offset": 1, "weekday": 2})), "candidate_group": b}
        ]})
    };
    // 번호가 달라도 같은 묶음이면 정답
    assert_eq!(score(expected.clone(), grouped(json!(7), json!(7))), Ok(()));
    assert!(score(expected.clone(), grouped(json!(1), json!(2))).is_err());
    assert!(score(expected, grouped(Value::Null, Value::Null)).is_err());
}

#[test]
fn assembly_failure_is_wrong() {
    let expected = case(json!({"events": [{"start": "2026-10-20"}]}));
    let output =
        json!({"events": [{"title": null, "start": rel(json!({"holiday": {"name": "없는날"}}))}]});
    assert!(score(expected, output).unwrap_err().contains("없는날"));
}
