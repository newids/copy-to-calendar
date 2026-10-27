//! 이벤트 조립 테스트 (ADR 0003). 기준일 2026-10-19(월) 09:00, 시간대 Asia/Seoul.

use c2c_core::assemble::{AssembleError, EventDraft, assemble};
use c2c_core::calendar::EventTime;
use c2c_core::context::ParseContext;
use c2c_core::holidays::HolidayTable;
use c2c_core::schema::{AmbiguityReason, ExtractionResult};
use serde_json::{Value, json};

fn context(time_zone: &str, reference: &str) -> ParseContext {
    serde_json::from_value(json!({"reference": reference, "time_zone": time_zone})).unwrap()
}

fn run_in(
    time_zone: &str,
    reference: &str,
    events: Value,
) -> Vec<Result<EventDraft, AssembleError>> {
    let result: ExtractionResult = serde_json::from_value(json!({"events": events})).unwrap();
    assemble(
        &result,
        &context(time_zone, reference),
        &HolidayTable::bundled(),
    )
}

fn run(events: Value) -> Vec<Result<EventDraft, AssembleError>> {
    run_in("Asia/Seoul", "2026-10-19T09:00:00", events)
}

fn rel(value: Value) -> Value {
    json!({"relative": value, "confidence": 0.9})
}

/// 시작~종료를 로컬 표기로. 종일은 "날짜..날짜", 시각은 RFC 3339.
fn span(draft: &EventDraft) -> String {
    match &draft.time {
        EventTime::AllDay { start, end } => format!("{start}..{end}"),
        EventTime::Timed { start, end, .. } => {
            format!("{}..{}", start.to_rfc3339(), end.to_rfc3339())
        }
    }
}

fn reasons(draft: &EventDraft) -> Vec<AmbiguityReason> {
    draft.ambiguities.iter().map(|a| a.reason).collect()
}

#[test]
fn prd_examples() {
    let drafts = run(json!([
        {"title": null, "start": rel(json!({"week_offset": 1, "weekday": 4, "time_word": "저녁", "hour": 7})),
         "location": "강남역 2번 출구"},
        {"title": "학부모 설명회", "start": rel(json!({"month": 10, "day": 24, "time_word": "오전", "hour": 10})),
         "end": rel(json!({"hour": 12})), "location": "3층 강당"},
        {"title": null, "start": rel(json!({"day_offset": 1, "time_word": "점심"}))}
    ]));
    let spans: Vec<String> = drafts.iter().map(|d| span(d.as_ref().unwrap())).collect();
    assert_eq!(
        spans,
        [
            "2026-10-29T19:00:00+09:00..2026-10-29T20:00:00+09:00",
            "2026-10-24T10:00:00+09:00..2026-10-24T12:00:00+09:00",
            "2026-10-20T12:00:00+09:00..2026-10-20T13:00:00+09:00",
        ]
    );
    let second = drafts[1].as_ref().unwrap();
    assert_eq!(second.title.as_deref(), Some("학부모 설명회"));
    assert_eq!(second.location.as_deref(), Some("3층 강당"));
    match &second.time {
        EventTime::Timed { time_zone, .. } => assert_eq!(time_zone, "Asia/Seoul"),
        other => panic!("{other:?}"),
    }
}

#[test]
fn default_duration_is_configurable() {
    let mut ctx = context("Asia/Seoul", "2026-10-19T09:00:00");
    ctx.defaults.duration_minutes = 30;
    let result: ExtractionResult = serde_json::from_value(json!({"events": [
        {"title": "x", "start": rel(json!({"day_offset": 1, "hour": 10}))}
    ]}))
    .unwrap();
    let drafts = assemble(&result, &ctx, &HolidayTable::default());
    assert_eq!(
        span(drafts[0].as_ref().unwrap()),
        "2026-10-20T10:00:00+09:00..2026-10-20T10:30:00+09:00"
    );
}

#[test]
fn all_day() {
    let drafts = run(json!([
        // 시각 표현이 없으면 종일
        {"title": "x", "start": rel(json!({"day_offset": 1}))},
        // 여러 날 종일
        {"title": "x", "start": rel(json!({"month": 11, "day": 2})), "end": rel(json!({"month": 11, "day": 4}))},
        // 모델이 종일이라고 하면 시각은 버린다
        {"title": "x", "start": rel(json!({"day_offset": 1, "hour": 15})), "all_day": true},
        // 종료가 시작보다 앞서면 하루짜리
        {"title": "x", "start": rel(json!({"day_offset": 2})), "end": rel(json!({"day_offset": 1}))},
        // 공휴일 (내장 데이터)
        {"title": "x", "start": rel(json!({"holiday": {"name": "추석", "whole_period": true}})),
         "end": rel(json!({"holiday": {"name": "추석", "day_offset": 1}})), "all_day": true}
    ]));
    let spans: Vec<String> = drafts.iter().map(|d| span(d.as_ref().unwrap())).collect();
    assert_eq!(
        spans,
        [
            "2026-10-20..2026-10-20",
            "2026-11-02..2026-11-04",
            "2026-10-20..2026-10-20",
            "2026-10-21..2026-10-21",
            "2027-09-14..2027-09-16",
        ]
    );
}

#[test]
fn ends() {
    let drafts = run(json!([
        // 자정을 넘는 종료 (해석기가 다음 날로)
        {"title": "x", "start": rel(json!({"day_offset": 1, "time_word": "밤", "hour": 11})), "end": rel(json!({"hour": 1}))},
        // explicit으로 같은 날짜에 적힌 자정 넘는 종료
        {"title": "x", "start": {"explicit": {"date": "2026-10-20", "time": "23:00"}, "confidence": 0.9},
         "end": {"explicit": {"date": "2026-10-20", "time": "01:00"}, "confidence": 0.9}},
        // 자정을 넘겨도 12시간이 넘으면 믿지 않는다: 기본 길이 + 기간 불명
        {"title": "x", "start": {"explicit": {"date": "2026-10-20", "time": "10:00"}, "confidence": 0.9},
         "end": {"explicit": {"date": "2026-10-20", "time": "01:00"}, "confidence": 0.9}},
        // 종료에 날짜만 있으면 기본 길이 + 기간 불명
        {"title": "x", "start": rel(json!({"day_offset": 1, "hour": 15})), "end": rel(json!({"day_offset": 3}))}
    ]));
    let drafts: Vec<&EventDraft> = drafts.iter().map(|d| d.as_ref().unwrap()).collect();
    assert_eq!(
        span(drafts[0]),
        "2026-10-20T23:00:00+09:00..2026-10-21T01:00:00+09:00"
    );
    assert!(drafts[0].ambiguities.is_empty());
    assert_eq!(
        span(drafts[1]),
        "2026-10-20T23:00:00+09:00..2026-10-21T01:00:00+09:00"
    );
    assert_eq!(
        span(drafts[2]),
        "2026-10-20T10:00:00+09:00..2026-10-20T11:00:00+09:00"
    );
    assert_eq!(reasons(drafts[2]), [AmbiguityReason::DurationUnknown]);
    assert_eq!(drafts[2].ambiguities[0].alternatives, ["2026-10-20T01:00"]);
    assert_eq!(
        span(drafts[3]),
        "2026-10-20T15:00:00+09:00..2026-10-20T16:00:00+09:00"
    );
    assert_eq!(reasons(drafts[3]), [AmbiguityReason::DurationUnknown]);
    assert_eq!(drafts[3].ambiguities[0].alternatives, ["2026-10-22"]);
}

#[test]
fn ambiguities_are_merged() {
    let drafts = run(json!([{
        "title": "x",
        "start": rel(json!({"month": 10, "day": 18, "hour": 3})),
        "ambiguities": [
            {"field": "location", "reason": "duration_unknown", "alternatives": []},
            // 코드가 판단하는 플래그는 모델이 내도 버린다
            {"field": "start.date", "reason": "year_missing", "alternatives": ["2030-01-01"]}
        ]
    }]));
    let draft = drafts[0].as_ref().unwrap();
    assert_eq!(
        reasons(draft),
        [
            AmbiguityReason::DurationUnknown,
            AmbiguityReason::YearMissing,
            AmbiguityReason::AmPmUnknown
        ]
    );
    assert_eq!(draft.ambiguities[1].alternatives, ["2027-10-18"]);
}

#[test]
fn candidate_groups() {
    let drafts = run(json!([
        {"title": null, "start": rel(json!({"week_offset": 0, "weekday": 5})), "candidate_group": 1},
        {"title": null, "start": rel(json!({"week_offset": 1, "weekday": 2})), "candidate_group": 1},
        {"title": "혼자", "start": rel(json!({"day_offset": 1, "hour": 15})), "candidate_group": 2},
        {"title": "그룹 없음", "start": rel(json!({"day_offset": 1, "hour": 16}))}
    ]));
    let drafts: Vec<&EventDraft> = drafts.iter().map(|d| d.as_ref().unwrap()).collect();
    assert_eq!(reasons(drafts[0]), [AmbiguityReason::MultipleCandidates]);
    assert_eq!(drafts[0].ambiguities[0].alternatives, ["2026-10-27"]);
    assert_eq!(drafts[1].ambiguities[0].alternatives, ["2026-10-23"]);
    assert!(
        drafts[2].ambiguities.is_empty(),
        "한 건뿐인 그룹은 후보가 아니다"
    );
    assert!(drafts[3].ambiguities.is_empty());
}

#[test]
fn failures_are_per_event() {
    let drafts = run(json!([
        {"title": "x", "start": rel(json!({"holiday": {"name": "없는날"}}))},
        {"title": "y", "start": rel(json!({"day_offset": 1}))}
    ]));
    assert!(matches!(
        drafts[0],
        Err(AssembleError::Resolve { field: "start", .. })
    ));
    assert!(drafts[1].is_ok());
}

#[test]
fn no_event_found() {
    let result: ExtractionResult = serde_json::from_value(json!({
        "no_event_found": true,
        "events": [{"title": "x", "start": rel(json!({"day_offset": 1}))}]
    }))
    .unwrap();
    let ctx = context("Asia/Seoul", "2026-10-19T09:00:00");
    assert!(assemble(&result, &ctx, &HolidayTable::default()).is_empty());
}

#[test]
fn time_zones() {
    let event = |date: &str, time: &str| json!([{"title": "x", "start": {"explicit": {"date": date, "time": time}, "confidence": 0.9}}]);
    // 서머타임 시작(2026-03-08 02:00 → 03:00)으로 없는 시각
    let gap = run_in(
        "America/New_York",
        "2026-03-01T09:00:00",
        event("2026-03-08", "02:30"),
    );
    assert!(matches!(
        gap[0],
        Err(AssembleError::NonexistentLocalTime(_))
    ));
    // 서머타임 끝(2026-11-01)으로 두 번 있는 01:30 → 이른 쪽(-04:00)
    let twice = run_in(
        "America/New_York",
        "2026-10-19T09:00:00",
        event("2026-11-01", "01:30"),
    );
    assert!(span(twice[0].as_ref().unwrap()).starts_with("2026-11-01T01:30:00-04:00"));
    let unknown = run_in(
        "Mars/Olympus",
        "2026-10-19T09:00:00",
        event("2026-10-20", "10:00"),
    );
    assert_eq!(
        unknown[0],
        Err(AssembleError::TimeZone("Mars/Olympus".into()))
    );
}
