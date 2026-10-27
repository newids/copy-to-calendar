//! 상대 날짜 해석 표 기반 테스트 (PRD 5.3, ADR 0002).
//!
//! 기준일은 따로 적지 않으면 2026-10-19(월) 09:00이다. PRD 3절 예시와 같은 기준일이다.

use c2c_core::context::{ParseContext, WeekRules, WorkingHours};
use c2c_core::holidays::{Holiday, HolidayTable};
use c2c_core::relative::{ResolveError, Resolved, Resolver};
use c2c_core::schema::{AmbiguityReason, DateTimeField, IsoWeekday, RelativeExpr};
use chrono::{NaiveDate, NaiveDateTime, NaiveTime, Weekday};
use serde_json::{Value, json};

const MONDAY: &str = "2026-10-19";

fn context(reference: &str) -> ParseContext {
    let reference =
        NaiveDateTime::parse_from_str(&format!("{reference}T09:00"), "%Y-%m-%dT%H:%M").unwrap();
    serde_json::from_value(json!({"reference": reference, "time_zone": "Asia/Seoul"})).unwrap()
}

fn early_working_hours(context: &mut ParseContext) {
    context.working_hours = WorkingHours {
        start: NaiveTime::from_hms_opt(7, 0, 0).unwrap(),
        end: NaiveTime::from_hms_opt(16, 0, 0).unwrap(),
    };
}

fn relative(value: Value) -> DateTimeField {
    DateTimeField {
        explicit: None,
        relative: Some(serde_json::from_value::<RelativeExpr>(value).unwrap()),
        confidence: 0.9,
    }
}

fn resolve(context: &ParseContext, value: Value) -> Result<Resolved, ResolveError> {
    Resolver::new(context, &HolidayTable::default()).resolve(&relative(value), "start")
}

fn show(r: &Resolved) -> String {
    match r.time {
        Some(t) => format!("{}T{}", r.date, t.format("%H:%M")),
        None => r.date.to_string(),
    }
}

fn flags(r: &Resolved) -> Vec<(AmbiguityReason, String)> {
    r.ambiguities
        .iter()
        .map(|a| (a.reason, a.alternatives.join(",")))
        .collect()
}

use AmbiguityReason::{AmPmUnknown, YearMissing};

/// (설명, 기준일, relative, 기대값, 기대 플래그)
type Row = (
    &'static str,
    &'static str,
    Value,
    &'static str,
    &'static [(AmbiguityReason, &'static str)],
);

fn run(rows: Vec<Row>, setup: impl Fn(&mut ParseContext)) {
    for (name, reference, value, expected, expected_flags) in rows {
        let mut ctx = context(reference);
        setup(&mut ctx);
        let r = resolve(&ctx, value).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert_eq!(show(&r), expected, "{name}");
        let want: Vec<_> = expected_flags
            .iter()
            .map(|(reason, alt)| (*reason, alt.to_string()))
            .collect();
        assert_eq!(flags(&r), want, "{name} 플래그");
    }
}

#[test]
fn day_offsets() {
    run(
        vec![
            ("오늘", MONDAY, json!({"day_offset": 0}), "2026-10-19", &[]),
            (
                "내일 점심 (PRD 3절)",
                MONDAY,
                json!({"day_offset": 1, "time_word": "점심"}),
                "2026-10-20T12:00",
                &[],
            ),
            ("모레", MONDAY, json!({"day_offset": 2}), "2026-10-21", &[]),
            ("글피", MONDAY, json!({"day_offset": 3}), "2026-10-22", &[]),
            (
                "월말 넘김",
                "2026-10-30",
                json!({"day_offset": 3}),
                "2026-11-02",
                &[],
            ),
        ],
        |_| {},
    );
}

#[test]
fn weeks() {
    run(
        vec![
            (
                "이번 주 금",
                MONDAY,
                json!({"week_offset": 0, "weekday": 5}),
                "2026-10-23",
                &[],
            ),
            (
                "이번 주 금, 기준일 토 → 다음 주",
                "2026-10-24",
                json!({"week_offset": 0, "weekday": 5}),
                "2026-10-30",
                &[],
            ),
            (
                "이번 주 금, 기준일 일 → 다음 주",
                "2026-10-25",
                json!({"week_offset": 0, "weekday": 5}),
                "2026-10-30",
                &[],
            ),
            (
                "이번 주 일, 기준일 토 → 아직 안 지남",
                "2026-10-24",
                json!({"week_offset": 0, "weekday": 7}),
                "2026-10-25",
                &[],
            ),
            (
                "담주 목 저녁 7시 (PRD 3절)",
                MONDAY,
                json!({"week_offset": 1, "weekday": 4, "time_word": "저녁", "hour": 7}),
                "2026-10-29T19:00",
                &[],
            ),
            (
                "담주 목, 기준일 일",
                "2026-10-25",
                json!({"week_offset": 1, "weekday": 4}),
                "2026-10-29",
                &[],
            ),
            (
                "다담주",
                MONDAY,
                json!({"week_offset": 2}),
                "2026-11-02",
                &[],
            ),
            (
                "이번 주 (요일 없음) → 오늘",
                "2026-10-21",
                json!({"week_offset": 0}),
                "2026-10-21",
                &[],
            ),
            (
                "금요일에 → 다가오는 금",
                MONDAY,
                json!({"weekday": 5}),
                "2026-10-23",
                &[],
            ),
            (
                "금요일에, 기준일 금 → 당일",
                "2026-10-23",
                json!({"weekday": 5}),
                "2026-10-23",
                &[],
            ),
            (
                "월요일에, 기준일 화 → 다음 월",
                "2026-10-20",
                json!({"weekday": 1}),
                "2026-10-26",
                &[],
            ),
            (
                "담주 월 다음날",
                MONDAY,
                json!({"week_offset": 1, "weekday": 1, "day_offset": 1}),
                "2026-10-27",
                &[],
            ),
        ],
        |_| {},
    );
}

#[test]
fn week_rules_are_configurable() {
    run(
        vec![
            (
                "주 시작 일요일: 담주 목, 기준일 일",
                "2026-10-25",
                json!({"week_offset": 1, "weekday": 4}),
                "2026-11-05",
                &[],
            ),
            (
                "주 시작 일요일: 이번 주 금, 기준일 일",
                "2026-10-25",
                json!({"week_offset": 0, "weekday": 5}),
                "2026-10-30",
                &[],
            ),
        ],
        |c| c.week.start = IsoWeekday::from(Weekday::Sun),
    );
    run(
        vec![(
            "주말 넘김 끔: 이번 주 금, 기준일 토",
            "2026-10-24",
            json!({"week_offset": 0, "weekday": 5}),
            "2026-10-23",
            &[],
        )],
        |c| {
            c.week = WeekRules {
                weekend_rolls_to_next_week: false,
                ..WeekRules::default()
            }
        },
    );
}

#[test]
fn days_of_month_and_missing_year() {
    run(
        vec![
            ("24일", MONDAY, json!({"day": 24}), "2026-10-24", &[]),
            (
                "24일, 이미 지남 → 다음 달",
                "2026-10-25",
                json!({"day": 24}),
                "2026-11-24",
                &[],
            ),
            (
                "31일, 11월에 없음 → 12월",
                "2026-11-05",
                json!({"day": 31}),
                "2026-12-31",
                &[],
            ),
            (
                "10/24 (PRD 3절)",
                MONDAY,
                json!({"month": 10, "day": 24, "time_word": "오전", "hour": 10}),
                "2026-10-24T10:00",
                &[],
            ),
            (
                "10/24, 하루 지남 → 올해 + 플래그",
                "2026-10-25",
                json!({"month": 10, "day": 24}),
                "2026-10-24",
                &[(YearMissing, "2027-10-24")],
            ),
            (
                "3/5, 6개월 넘게 지남 → 내년",
                MONDAY,
                json!({"month": 3, "day": 5}),
                "2027-03-05",
                &[(YearMissing, "2026-03-05")],
            ),
            (
                "4/19, 정확히 6개월 전 → 올해",
                MONDAY,
                json!({"month": 4, "day": 19}),
                "2026-04-19",
                &[(YearMissing, "2027-04-19")],
            ),
            (
                "1/10, 해 넘김",
                "2026-12-20",
                json!({"month": 1, "day": 10}),
                "2027-01-10",
                &[(YearMissing, "2026-01-10")],
            ),
            (
                "다음 달 24일",
                MONDAY,
                json!({"month_offset": 1, "day": 24}),
                "2026-11-24",
                &[],
            ),
            (
                "11월 (일 없음) → 1일",
                MONDAY,
                json!({"month": 11}),
                "2026-11-01",
                &[],
            ),
            (
                "10월 (이번 달) → 오늘",
                MONDAY,
                json!({"month": 10}),
                "2026-10-19",
                &[],
            ),
        ],
        |_| {},
    );
}

#[test]
fn month_periods_resolve_to_earliest_day() {
    run(
        vec![
            (
                "다음 달 초",
                MONDAY,
                json!({"month_offset": 1, "month_period": "월초"}),
                "2026-11-01",
                &[],
            ),
            (
                "다음 달 상순",
                MONDAY,
                json!({"month_offset": 1, "month_period": "상순"}),
                "2026-11-01",
                &[],
            ),
            (
                "다음 달 중순",
                MONDAY,
                json!({"month_offset": 1, "month_period": "중순"}),
                "2026-11-11",
                &[],
            ),
            (
                "다음 달 하순",
                MONDAY,
                json!({"month_offset": 1, "month_period": "하순"}),
                "2026-11-21",
                &[],
            ),
            (
                "다음 달 말",
                MONDAY,
                json!({"month_offset": 1, "month_period": "월말"}),
                "2026-11-21",
                &[],
            ),
            (
                "이번 달 하순",
                MONDAY,
                json!({"month_offset": 0, "month_period": "하순"}),
                "2026-10-21",
                &[],
            ),
            (
                "중순, 기준일이 구간 안 → 오늘",
                MONDAY,
                json!({"month_period": "중순"}),
                "2026-10-19",
                &[],
            ),
            (
                "월말 (월 없음)",
                MONDAY,
                json!({"month_period": "월말"}),
                "2026-10-21",
                &[],
            ),
            (
                "상순, 이미 지남 → 다음 달",
                MONDAY,
                json!({"month_period": "상순"}),
                "2026-11-01",
                &[],
            ),
            (
                "11월 하순",
                MONDAY,
                json!({"month": 11, "month_period": "하순"}),
                "2026-11-21",
                &[],
            ),
            (
                "10월 하순, 기준일이 구간 안",
                "2026-10-25",
                json!({"month": 10, "month_period": "하순"}),
                "2026-10-25",
                &[],
            ),
        ],
        |_| {},
    );
}

#[test]
fn times_with_words() {
    let day = json!({"day_offset": 1});
    let with = |extra: Value| {
        let mut v = day.clone();
        v.as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        v
    };
    run(
        vec![
            (
                "저녁 7시",
                MONDAY,
                with(json!({"time_word": "저녁", "hour": 7})),
                "2026-10-20T19:00",
                &[],
            ),
            (
                "오전 10시",
                MONDAY,
                with(json!({"time_word": "오전", "hour": 10})),
                "2026-10-20T10:00",
                &[],
            ),
            (
                "오후 2시 반",
                MONDAY,
                with(json!({"time_word": "오후", "hour": 2, "minute": 30})),
                "2026-10-20T14:30",
                &[],
            ),
            (
                "오후 12시",
                MONDAY,
                with(json!({"time_word": "오후", "hour": 12})),
                "2026-10-20T12:00",
                &[],
            ),
            (
                "오전 12시 → 자정",
                MONDAY,
                with(json!({"time_word": "오전", "hour": 12})),
                "2026-10-20T00:00",
                &[],
            ),
            (
                "점심 1시",
                MONDAY,
                with(json!({"time_word": "점심", "hour": 1})),
                "2026-10-20T13:00",
                &[],
            ),
            (
                "점심 12시",
                MONDAY,
                with(json!({"time_word": "점심", "hour": 12})),
                "2026-10-20T12:00",
                &[],
            ),
            (
                "새벽 2시",
                MONDAY,
                with(json!({"time_word": "새벽", "hour": 2})),
                "2026-10-20T02:00",
                &[],
            ),
            (
                "아침 8시",
                MONDAY,
                with(json!({"time_word": "아침", "hour": 8})),
                "2026-10-20T08:00",
                &[],
            ),
            (
                "밤 9시",
                MONDAY,
                with(json!({"time_word": "밤", "hour": 9})),
                "2026-10-20T21:00",
                &[],
            ),
            (
                "밤 12시 → 다음 날 0시",
                MONDAY,
                with(json!({"time_word": "밤", "hour": 12})),
                "2026-10-21T00:00",
                &[],
            ),
            (
                "밤 1시 → 다음 날 1시",
                MONDAY,
                with(json!({"time_word": "밤", "hour": 1})),
                "2026-10-21T01:00",
                &[],
            ),
            (
                "24시간제 15시는 말과 무관",
                MONDAY,
                with(json!({"time_word": "오전", "hour": 15})),
                "2026-10-20T15:00",
                &[],
            ),
        ],
        |_| {},
    );
}

#[test]
fn time_word_defaults() {
    run(
        vec![
            (
                "새벽",
                MONDAY,
                json!({"day_offset": 1, "time_word": "새벽"}),
                "2026-10-20T05:00",
                &[],
            ),
            (
                "아침",
                MONDAY,
                json!({"day_offset": 1, "time_word": "아침"}),
                "2026-10-20T08:00",
                &[],
            ),
            (
                "오전",
                MONDAY,
                json!({"day_offset": 1, "time_word": "오전"}),
                "2026-10-20T10:00",
                &[],
            ),
            (
                "점심",
                MONDAY,
                json!({"day_offset": 1, "time_word": "점심"}),
                "2026-10-20T12:00",
                &[],
            ),
            (
                "오후",
                MONDAY,
                json!({"day_offset": 1, "time_word": "오후"}),
                "2026-10-20T13:00",
                &[],
            ),
            (
                "저녁",
                MONDAY,
                json!({"day_offset": 1, "time_word": "저녁"}),
                "2026-10-20T19:00",
                &[],
            ),
            (
                "밤",
                MONDAY,
                json!({"day_offset": 1, "time_word": "밤"}),
                "2026-10-20T19:00",
                &[],
            ),
            (
                "시각만, 날짜 없음 → 오늘",
                MONDAY,
                json!({"hour": 15}),
                "2026-10-19T15:00",
                &[],
            ),
        ],
        |_| {},
    );
}

#[test]
fn hours_without_am_pm() {
    run(
        vec![
            (
                "3시 → 오후 (1~6시)",
                MONDAY,
                json!({"hour": 3}),
                "2026-10-19T15:00",
                &[(AmPmUnknown, "03:00")],
            ),
            (
                "6시 → 오후",
                MONDAY,
                json!({"hour": 6}),
                "2026-10-19T18:00",
                &[(AmPmUnknown, "06:00")],
            ),
            (
                "12시 → 정오, 플래그 없음",
                MONDAY,
                json!({"hour": 12}),
                "2026-10-19T12:00",
                &[],
            ),
            ("0시", MONDAY, json!({"hour": 0}), "2026-10-19T00:00", &[]),
            (
                "7시, 기본 근무 09시 시작 전 → 오후",
                MONDAY,
                json!({"hour": 7}),
                "2026-10-19T19:00",
                &[(AmPmUnknown, "07:00")],
            ),
            (
                "8시 30분, 근무 전 → 오후",
                MONDAY,
                json!({"hour": 8, "minute": 30}),
                "2026-10-19T20:30",
                &[(AmPmUnknown, "08:30")],
            ),
            (
                "10시, 근무 중 → 오전",
                MONDAY,
                json!({"hour": 10}),
                "2026-10-19T10:00",
                &[(AmPmUnknown, "22:00")],
            ),
        ],
        |_| {},
    );
    run(
        vec![(
            "7시, 근무 07시 시작으로 설정 → 오전",
            MONDAY,
            json!({"hour": 7}),
            "2026-10-19T07:00",
            &[(AmPmUnknown, "19:00")],
        )],
        early_working_hours,
    );
}

#[test]
fn end_follows_start() {
    let ctx = context(MONDAY);
    let holidays = HolidayTable::default();
    let resolver = Resolver::new(&ctx, &holidays);
    let cases = [
        // (start relative, end relative, 기대 종료)
        (
            json!({"month": 10, "day": 24, "time_word": "오전", "hour": 10}),
            json!({"hour": 12}),
            "2026-10-24T12:00",
        ),
        (
            json!({"day_offset": 1, "time_word": "오후", "hour": 2}),
            json!({"hour": 4}),
            "2026-10-20T16:00",
        ),
        (
            json!({"day_offset": 1, "time_word": "오전", "hour": 11}),
            json!({"hour": 1}),
            "2026-10-20T13:00",
        ),
        (
            json!({"day_offset": 1, "time_word": "저녁", "hour": 7}),
            json!({"time_word": "밤", "hour": 10}),
            "2026-10-20T22:00",
        ),
        (
            json!({"day_offset": 1, "hour": 10}),
            json!({"day_offset": 2, "hour": 18}),
            "2026-10-21T18:00",
        ),
    ];
    for (start, end, expected) in cases {
        let s = resolver.resolve(&relative(start), "start").unwrap();
        let e = resolver.resolve_end(&relative(end), &s, "end").unwrap();
        assert_eq!(show(&e), expected);
        assert!(e.ambiguities.is_empty(), "{expected}: {:?}", e.ambiguities);
    }
}

#[test]
fn explicit_and_relative() {
    let ctx = context(MONDAY);
    let holidays = HolidayTable::default();
    let resolver = Resolver::new(&ctx, &holidays);
    let explicit = json!({"date": "2026-10-24", "time": "10:00"});

    let only: DateTimeField =
        serde_json::from_value(json!({"explicit": explicit, "confidence": 0.95})).unwrap();
    let r = resolver.resolve(&only, "start").unwrap();
    assert_eq!(show(&r), "2026-10-24T10:00");
    assert!(r.ambiguities.is_empty());

    let agree: DateTimeField = serde_json::from_value(json!({
        "explicit": explicit,
        "relative": {"week_offset": 0, "weekday": 6, "time_word": "오전", "hour": 10},
        "confidence": 0.95
    }))
    .unwrap();
    assert!(
        resolver
            .resolve(&agree, "start")
            .unwrap()
            .ambiguities
            .is_empty()
    );

    let differ: DateTimeField = serde_json::from_value(json!({
        "explicit": explicit,
        "relative": {"week_offset": 1, "weekday": 6, "time_word": "오전", "hour": 10},
        "confidence": 0.95
    }))
    .unwrap();
    let r = resolver.resolve(&differ, "start").unwrap();
    assert_eq!(show(&r), "2026-10-24T10:00", "원문 절대 날짜를 따른다");
    assert_eq!(
        r.ambiguities[0].reason,
        AmbiguityReason::ExplicitRelativeMismatch
    );
    assert_eq!(r.ambiguities[0].field, "start");
    assert_eq!(r.ambiguities[0].alternatives, ["2026-10-31T10:00"]);

    let empty: DateTimeField =
        serde_json::from_value(json!({"explicit": null, "relative": null, "confidence": 0.1}))
            .unwrap();
    assert_eq!(resolver.resolve(&empty, "start"), Err(ResolveError::Empty));
}

#[test]
fn holidays_resolve_from_table() {
    // 실제 공휴일 데이터가 아니라 해석 규칙만 보는 가상 테이블.
    let d = |s: &str| NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap();
    let table = HolidayTable::new(
        [
            "2026-10-27",
            "2026-10-28",
            "2026-10-29",
            "2027-10-15",
            "2027-10-16",
        ]
        .into_iter()
        .map(|s| Holiday {
            date: d(s),
            name: "가상연휴".into(),
        })
        .collect(),
    );
    let resolve_holiday = |reference: &str, value: Value| {
        let ctx = context(reference);
        Resolver::new(&ctx, &table)
            .resolve(&relative(value), "start")
            .map(|r| show(&r))
    };

    let holiday = |offset: i32| json!({"holiday": {"name": "가상연휴", "day_offset": offset}});
    let whole = json!({"holiday": {"name": "가상연휴", "whole_period": true}});
    // 3일 연휴의 당일은 가운데 날
    assert_eq!(resolve_holiday(MONDAY, holiday(0)).unwrap(), "2026-10-28");
    assert_eq!(resolve_holiday(MONDAY, holiday(-1)).unwrap(), "2026-10-27");
    assert_eq!(
        resolve_holiday(MONDAY, whole.clone()).unwrap(),
        "2026-10-27"
    );
    // 연휴 중이어도 그 연휴를 가리킨다
    assert_eq!(
        resolve_holiday("2026-10-28", holiday(0)).unwrap(),
        "2026-10-28"
    );
    // 지난 연휴는 건너뛴다. 2일 연휴의 당일은 첫날
    assert_eq!(
        resolve_holiday("2026-10-30", holiday(0)).unwrap(),
        "2027-10-15"
    );
    assert_eq!(
        resolve_holiday(MONDAY, json!({"holiday": {"name": "없는날"}})),
        Err(ResolveError::UnknownHoliday("없는날".into()))
    );
}

#[test]
fn bundled_holidays() {
    // core/data/holidays.json (한국천문연구원 특일 정보, 2026·2027년)
    let table = HolidayTable::bundled();
    let resolve_holiday = |reference: &str, value: Value| {
        let ctx = context(reference);
        Resolver::new(&ctx, &table)
            .resolve(&relative(value), "start")
            .map(|r| show(&r))
            .unwrap()
    };
    let new_year = "2026-01-10";
    let cases = [
        (new_year, json!({"holiday": {"name": "설날"}}), "2026-02-17"),
        (
            new_year,
            json!({"holiday": {"name": "설날", "day_offset": -1}}),
            "2026-02-16",
        ),
        (
            new_year,
            json!({"holiday": {"name": "설날", "day_offset": 1}}),
            "2026-02-18",
        ),
        (new_year, json!({"holiday": {"name": "추석"}}), "2026-09-25"),
        (
            new_year,
            json!({"holiday": {"name": "추석", "whole_period": true}}),
            "2026-09-24",
        ),
        (MONDAY, json!({"holiday": {"name": "설날"}}), "2027-02-07"),
        (
            MONDAY,
            json!({"holiday": {"name": "추석", "whole_period": true}}),
            "2027-09-14",
        ),
    ];
    for (reference, value, expected) in cases {
        assert_eq!(
            resolve_holiday(reference, value.clone()),
            expected,
            "{value}"
        );
    }
}

#[test]
fn invalid_values_are_errors() {
    let ctx = context(MONDAY);
    for value in [
        json!({"hour": 24}),
        json!({"hour": 9, "minute": 60}),
        json!({"month": 13, "day": 1}),
        json!({"month": 2, "day": 30}),
        json!({"day": 32}),
    ] {
        assert!(
            matches!(resolve(&ctx, value.clone()), Err(ResolveError::Invalid(_))),
            "{value}"
        );
    }
}
