use c2c_core::context::{ParseContext, TimeDefaults, WeekRules};
use c2c_core::schema::{IsoWeekday, TimeWord};
use chrono::{NaiveTime, Weekday};
use serde_json::json;

#[test]
fn defaults_follow_prd() {
    let week = WeekRules::default();
    assert_eq!(week.start.to_chrono(), Weekday::Mon);
    assert!(week.weekend_rolls_to_next_week);

    let times = TimeDefaults::default();
    assert_eq!(times.lunch, NaiveTime::from_hms_opt(12, 0, 0).unwrap());
    assert_eq!(times.evening, NaiveTime::from_hms_opt(19, 0, 0).unwrap());
    assert_eq!(times.am, NaiveTime::from_hms_opt(10, 0, 0).unwrap());
    assert_eq!(times.duration_minutes, 60);
}

#[test]
fn time_word_defaults() {
    let times = TimeDefaults::default();
    let at = |h| NaiveTime::from_hms_opt(h, 0, 0).unwrap();
    let cases = [
        (TimeWord::Dawn, 5),
        (TimeWord::Morning, 8),
        (TimeWord::Am, 10),
        (TimeWord::Lunch, 12),
        (TimeWord::Pm, 13),
        (TimeWord::Evening, 19),
        (TimeWord::Night, 19),
    ];
    for (word, hour) in cases {
        assert_eq!(times.time_for(word), at(hour), "{word:?}");
    }
}

#[test]
fn time_defaults_are_configurable() {
    let context: ParseContext = serde_json::from_value(json!({
        "reference": "2026-10-19T09:00:00",
        "time_zone": "Asia/Seoul",
        "defaults": {
            "dawn": "06:00", "morning": "07:30", "am": "10:00", "lunch": "12:30",
            "pm": "14:00", "evening": "18:00", "night": "21:00", "duration_minutes": 30
        }
    }))
    .unwrap();
    assert_eq!(
        context.defaults.time_for(TimeWord::Night),
        NaiveTime::from_hms_opt(21, 0, 0).unwrap()
    );
}

#[test]
fn week_start_is_configurable() {
    let context: ParseContext = serde_json::from_value(json!({
        "reference": "2026-10-19T09:00:00",
        "time_zone": "Asia/Seoul",
        "week": {"start": 7, "weekend_rolls_to_next_week": false}
    }))
    .unwrap();
    assert_eq!(context.week.start, IsoWeekday::from(Weekday::Sun));
    assert!(!context.week.weekend_rolls_to_next_week);
}

#[test]
fn omitted_settings_use_defaults() {
    let context: ParseContext = serde_json::from_value(json!({
        "reference": "2026-10-19T09:00:00",
        "time_zone": "Asia/Seoul"
    }))
    .unwrap();
    assert_eq!(context.week, WeekRules::default());
    assert_eq!(context.defaults, TimeDefaults::default());
}
