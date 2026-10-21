//! LLM 추출 스키마 (PRD 5.1).
//!
//! 클라우드·로컬 공통 스키마 하나다. 모델은 날짜마다 `explicit`(원문에 절대 날짜가 있을 때)과
//! `relative`(상대 표현일 때) 중 하나를 채우고, 날짜 확정은 항상 코드(PRD 5.3)가 한다.

use chrono::{NaiveDate, NaiveTime, Weekday};
use serde::{Deserialize, Serialize};

/// LLM 응답 최상위 객체.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExtractionResult {
    #[serde(default)]
    pub events: Vec<Event>,
    #[serde(default)]
    pub no_event_found: bool,
}

/// 메시지에서 추출한 일정 1건. 한 메시지에 일정이 여러 개면 `events`에 여러 건이 온다 (PRD 4.1).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Event {
    /// 원문 언어 유지. 추출하지 못하면 null (PRD 5.2).
    pub title: Option<String>,
    pub start: DateTimeField,
    /// 없으면 후처리에서 기본 길이(+1시간)를 적용한다 (PRD 4.2).
    #[serde(default)]
    pub end: Option<DateTimeField>,
    #[serde(default)]
    pub all_day: bool,
    #[serde(default)]
    pub location: Option<String>,
    #[serde(default)]
    pub attendees: Vec<String>,
    /// RFC 5545 RRULE 문자열, 예: `FREQ=WEEKLY;BYDAY=MO`.
    #[serde(default)]
    pub recurrence: Option<String>,
    /// 원문 발췌.
    #[serde(default)]
    pub notes: Option<String>,
    #[serde(default)]
    pub ambiguities: Vec<Ambiguity>,
}

/// 시작·종료 시각 하나. `explicit`과 `relative`가 둘 다 있으면 코드가 둘을 계산해 비교하고,
/// 다르면 [`AmbiguityReason::ExplicitRelativeMismatch`]를 세운다 (PRD 5.1).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DateTimeField {
    #[serde(default)]
    pub explicit: Option<ExplicitDateTime>,
    #[serde(default)]
    pub relative: Option<RelativeExpr>,
    /// 0.0 ~ 1.0. 카드 표시 임계값은 PRD 5.4.
    pub confidence: f64,
}

/// 원문에 있는 절대 날짜. ISO 8601 로컬 표기, 시간대는 코드가 사용자 설정값을 붙인다.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExplicitDateTime {
    pub date: NaiveDate,
    /// `"HH:MM"`. 종일 일정이면 null.
    #[serde(default, with = "hhmm_opt")]
    pub time: Option<NaiveTime>,
}

/// 상대 날짜·시간의 중간 표현 (PRD 5.1·5.2·5.3).
///
/// 모델은 계산하지 않고 원문 표현을 이 필드들로 옮기기만 한다. 필드는 조합해서 쓴다:
///
/// | 원문 | 표현 |
/// | --- | --- |
/// | 이번 주 금 | `week_offset: 0, weekday: 5` |
/// | 담주 목 / 다음 주 목 | `week_offset: 1, weekday: 4` |
/// | 다담주 | `week_offset: 2` |
/// | 오늘 / 내일 / 모레 / 글피 | `day_offset: 0 / 1 / 2 / 3` |
/// | 24일 (월 누락) | `day: 24` |
/// | 10/24 (연도 누락) | `month: 10, day: 24` |
/// | 다음 달 초 / 이번 달 하순 | `month_offset: 1, month_period: "월초"` / `month_offset: 0, month_period: "하순"` |
/// | 추석 연휴 / 설 전날 | `holiday: {name: "추석"}` / `holiday: {name: "설날", day_offset: -1}` |
/// | 저녁 7시 | `time_word: "저녁", hour: 7` |
/// | 7시 (오전·오후 누락) | `hour: 7` |
///
/// 주 시작은 월요일(ISO-8601)이다. 값의 해석 규칙은 PRD 5.3, 구현은 0-3 세션 몫이다.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RelativeExpr {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub day_offset: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub week_offset: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub weekday: Option<IsoWeekday>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub month_offset: Option<i32>,
    /// 연도 없이 적힌 월 (1~12).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub month: Option<u32>,
    /// 연·월 없이 적힌 일 (1~31).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub day: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub month_period: Option<MonthPeriod>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub holiday: Option<HolidayRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub time_word: Option<TimeWord>,
    /// 원문에 적힌 시. 12시간제일 수 있으며 오전·오후 확정은 `time_word`와 후처리가 한다.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hour: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub minute: Option<u32>,
}

/// ISO-8601 요일 번호. 1 = 월요일 … 7 = 일요일.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "u8", into = "u8")]
pub struct IsoWeekday(u8);

impl IsoWeekday {
    pub fn number(self) -> u8 {
        self.0
    }

    pub fn to_chrono(self) -> Weekday {
        Weekday::try_from(self.0 - 1).expect("1..=7 범위는 생성 시 검증됨")
    }
}

impl TryFrom<u8> for IsoWeekday {
    type Error = String;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        if (1..=7).contains(&value) {
            Ok(Self(value))
        } else {
            Err(format!("weekday는 1(월)~7(일)이어야 한다: {value}"))
        }
    }
}

impl From<IsoWeekday> for u8 {
    fn from(value: IsoWeekday) -> Self {
        value.0
    }
}

impl From<Weekday> for IsoWeekday {
    fn from(value: Weekday) -> Self {
        Self(value.number_from_monday() as u8)
    }
}

/// 월 안의 구간 표현 (PRD 5.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum MonthPeriod {
    #[serde(rename = "월초")]
    Start,
    #[serde(rename = "상순")]
    Early,
    #[serde(rename = "중순")]
    Middle,
    #[serde(rename = "하순")]
    Late,
    #[serde(rename = "월말")]
    End,
}

/// 공휴일·명절 기준 표현 (PRD 5.3). 공휴일 테이블과 이름 목록은 후처리 단계에서 정한다.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HolidayRef {
    pub name: String,
    /// 공휴일 당일 기준 일수. "설 전날"은 -1.
    #[serde(default)]
    pub day_offset: i32,
}

/// 시간대를 가리키는 말. 기본 시각은 [`crate::context::TimeDefaults`]가 정한다 (PRD 4.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TimeWord {
    #[serde(rename = "새벽")]
    Dawn,
    #[serde(rename = "아침")]
    Morning,
    #[serde(rename = "오전")]
    Am,
    #[serde(rename = "점심")]
    Lunch,
    #[serde(rename = "오후")]
    Pm,
    #[serde(rename = "저녁")]
    Evening,
    #[serde(rename = "밤")]
    Night,
}

/// 모호성 플래그. 비어 있지 않으면 카드에서 해당 필드를 하이라이트한다 (PRD 5.1·5.4).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Ambiguity {
    /// 점으로 구분한 필드 경로, 예: `start.date`, `end.time`.
    pub field: String,
    pub reason: AmbiguityReason,
    /// 대안 값. PRD 5.1 예시처럼 ISO 8601 문자열.
    #[serde(default)]
    pub alternatives: Vec<String>,
}

/// PRD 4.2의 모호성 종류 + PRD 5.1의 explicit·relative 불일치.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AmbiguityReason {
    /// 연도 누락
    YearMissing,
    /// 오전/오후 불명
    AmPmUnknown,
    /// 기간 불명
    DurationUnknown,
    /// 날짜 후보 복수
    MultipleCandidates,
    /// explicit과 relative를 각각 계산한 결과가 다름. 모델이 아니라 후처리 코드가 세운다
    ExplicitRelativeMismatch,
}

/// `Option<NaiveTime>`을 PRD 표기 `"HH:MM"`으로 직렬화한다. 역직렬화는 초 단위 표기도 받는다.
mod hhmm_opt {
    use chrono::NaiveTime;
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(value: &Option<NaiveTime>, s: S) -> Result<S::Ok, S::Error> {
        match value {
            Some(t) => s.serialize_str(&t.format("%H:%M").to_string()),
            None => s.serialize_none(),
        }
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Option<NaiveTime>, D::Error> {
        Option::<NaiveTime>::deserialize(d)
    }
}
