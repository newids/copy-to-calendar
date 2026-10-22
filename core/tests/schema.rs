//! PRD 5.1 예시 JSON이 스키마 타입과 맞는지 검증한다.

use c2c_core::schema::{
    AmbiguityReason, DateTimeField, ExtractionResult, IsoWeekday, MonthPeriod, RelativeExpr,
    TimeWord,
};
use chrono::{NaiveDate, NaiveTime, Weekday};
use serde_json::json;

const PRD_EXPLICIT: &str = r#"{
  "events": [
    {
      "title": "학부모 설명회",
      "start": {
        "explicit": {"date": "2026-10-24", "time": "10:00"},
        "relative": null,
        "confidence": 0.95
      },
      "end": {
        "explicit": {"date": "2026-10-24", "time": "12:00"},
        "relative": null,
        "confidence": 0.95
      },
      "all_day": false,
      "location": "3층 강당",
      "attendees": [],
      "recurrence": null,
      "notes": "원문 발췌",
      "ambiguities": []
    }
  ],
  "no_event_found": false
}"#;

#[test]
fn prd_explicit_example_parses() {
    let result: ExtractionResult = serde_json::from_str(PRD_EXPLICIT).unwrap();
    assert!(!result.no_event_found);
    let event = &result.events[0];
    assert_eq!(event.title.as_deref(), Some("학부모 설명회"));
    let start = event.start.explicit.as_ref().unwrap();
    assert_eq!(start.date, NaiveDate::from_ymd_opt(2026, 10, 24).unwrap());
    assert_eq!(start.time, NaiveTime::from_hms_opt(10, 0, 0));
    assert!(event.start.relative.is_none());
    assert_eq!(event.location.as_deref(), Some("3층 강당"));
    assert!(event.ambiguities.is_empty());
    assert!(event.candidate_group.is_none());
}

#[test]
fn prd_year_missing_example_is_relative() {
    let field: DateTimeField = serde_json::from_value(json!({
        "explicit": null,
        "relative": {"month": 10, "day": 24, "time_word": "오전", "hour": 10},
        "confidence": 0.95
    }))
    .unwrap();
    assert!(field.explicit.is_none());
    let rel = field.relative.unwrap();
    assert_eq!(
        (rel.month, rel.day, rel.hour),
        (Some(10), Some(24), Some(10))
    );
    assert_eq!(rel.time_word, Some(TimeWord::Am));
}

#[test]
fn prd_candidate_group_example_parses() {
    let result: ExtractionResult = serde_json::from_value(json!({"events": [
        {"title": null, "start": {"explicit": null, "relative": {"week_offset": 0, "weekday": 5}, "confidence": 0.6}, "candidate_group": 1},
        {"title": null, "start": {"explicit": null, "relative": {"week_offset": 1, "weekday": 2}, "confidence": 0.6}, "candidate_group": 1}
    ]}))
    .unwrap();
    let groups: Vec<_> = result.events.iter().map(|e| e.candidate_group).collect();
    assert_eq!(groups, [Some(1), Some(1)]);
}

#[test]
fn ambiguity_parses() {
    let result: ExtractionResult = serde_json::from_value(json!({"events": [{
        "title": "x",
        "start": {"relative": {"month": 10, "day": 24}, "confidence": 0.95},
        "ambiguities": [{"field": "start.date", "reason": "year_missing", "alternatives": ["2027-10-24"]}]
    }]}))
    .unwrap();
    let ambiguity = &result.events[0].ambiguities[0];
    assert_eq!(ambiguity.reason, AmbiguityReason::YearMissing);
    assert_eq!(ambiguity.alternatives, ["2027-10-24"]);
}

#[test]
fn prd_relative_example_parses() {
    let field: DateTimeField = serde_json::from_value(json!({
        "explicit": null,
        "relative": {"week_offset": 1, "weekday": 4, "time_word": "저녁", "hour": 7},
        "confidence": 0.9
    }))
    .unwrap();
    let rel = field.relative.unwrap();
    assert_eq!(rel.week_offset, Some(1));
    assert_eq!(rel.weekday.unwrap().to_chrono(), Weekday::Thu);
    assert_eq!(rel.time_word, Some(TimeWord::Evening));
    assert_eq!(rel.hour, Some(7));
    assert_eq!(rel.day_offset, None);
}

#[test]
fn roundtrip_keeps_prd_format() {
    let result: ExtractionResult = serde_json::from_str(PRD_EXPLICIT).unwrap();
    let value = serde_json::to_value(&result).unwrap();
    assert_eq!(value["events"][0]["start"]["explicit"]["time"], "10:00");
    assert_eq!(
        value["events"][0]["start"]["explicit"]["date"],
        "2026-10-24"
    );
    let back: ExtractionResult = serde_json::from_value(value).unwrap();
    assert_eq!(back, result);
}

#[test]
fn relative_serializes_only_present_fields() {
    let rel = RelativeExpr {
        day_offset: Some(2),
        ..Default::default()
    };
    assert_eq!(
        serde_json::to_value(&rel).unwrap(),
        json!({"day_offset": 2})
    );
}

#[test]
fn relative_accepts_explicit_nulls() {
    // 구조화 출력은 모든 필드를 null로 채워 보내기도 한다.
    let rel: RelativeExpr = serde_json::from_value(json!({
        "day_offset": null, "week_offset": null, "weekday": null, "month_offset": null,
        "month": 10, "day": 24, "month_period": null, "holiday": null,
        "time_word": null, "hour": null, "minute": null
    }))
    .unwrap();
    assert_eq!((rel.month, rel.day), (Some(10), Some(24)));
}

#[test]
fn korean_words_map_to_enums() {
    let rel: RelativeExpr = serde_json::from_value(json!({
        "month_offset": 1, "month_period": "하순",
        "holiday": {"name": "설날", "day_offset": -1},
        "time_word": "점심"
    }))
    .unwrap();
    assert_eq!(rel.month_period, Some(MonthPeriod::Late));
    assert_eq!(rel.holiday.unwrap().day_offset, -1);
    assert_eq!(rel.time_word, Some(TimeWord::Lunch));
}

#[test]
fn weekday_out_of_range_is_rejected() {
    assert!(serde_json::from_value::<IsoWeekday>(json!(0)).is_err());
    assert!(serde_json::from_value::<IsoWeekday>(json!(8)).is_err());
    let sunday: IsoWeekday = serde_json::from_value(json!(7)).unwrap();
    assert_eq!(sunday.to_chrono(), Weekday::Sun);
    assert_eq!(IsoWeekday::from(Weekday::Mon).number(), 1);
}

#[test]
fn no_event_found_with_minimal_fields() {
    let result: ExtractionResult = serde_json::from_value(json!({"no_event_found": true})).unwrap();
    assert!(result.no_event_found);
    assert!(result.events.is_empty());
}

#[test]
fn event_defaults_for_omitted_optional_fields() {
    let result: ExtractionResult = serde_json::from_value(json!({
        "events": [{"title": null, "start": {"relative": {"day_offset": 1, "time_word": "점심"}, "confidence": 0.9}}]
    }))
    .unwrap();
    let event = &result.events[0];
    assert!(event.title.is_none());
    assert!(event.end.is_none());
    assert!(!event.all_day);
    assert!(event.attendees.is_empty() && event.ambiguities.is_empty());
}
