//! 파싱 컨텍스트: 프롬프트에 넣고 후처리에서 쓰는 사용자 기준값 (PRD 4.2·5.2).

use chrono::{NaiveDateTime, NaiveTime};
use serde::{Deserialize, Serialize};

use crate::schema::IsoWeekday;

/// 추출 요청 1건에 붙는 컨텍스트.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ParseContext {
    /// "오늘"의 기준 일시. 메시지 수신 시각이 있으면 그것, 없으면 붙여넣기 시점 (PRD 5.2).
    pub reference: NaiveDateTime,
    /// 사용자 시간대 (IANA 이름, 예: `Asia/Seoul`).
    pub time_zone: String,
    #[serde(default)]
    pub working_hours: Option<WorkingHours>,
    #[serde(default)]
    pub defaults: TimeDefaults,
    #[serde(default)]
    pub week: WeekRules,
    /// 최근 등록한 장소, 최대 10개 (PRD 5.2).
    #[serde(default)]
    pub recent_locations: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkingHours {
    pub start: NaiveTime,
    pub end: NaiveTime,
}

/// 주 경계 규칙. 설정에서 변경 가능 (PRD 5.3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct WeekRules {
    /// 주 시작 요일. "이번 주"·"담주"의 경계가 된다
    pub start: IsoWeekday,
    /// 기준일이 토·일일 때 "이번 주 금"을 다음 주로 넘긴다
    pub weekend_rolls_to_next_week: bool,
}

impl Default for WeekRules {
    fn default() -> Self {
        Self {
            start: IsoWeekday::MONDAY,
            weekend_rolls_to_next_week: true,
        }
    }
}

/// 기본 시간값. 설정에서 변경 가능 (PRD 4.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct TimeDefaults {
    /// "점심"
    pub lunch: NaiveTime,
    /// "저녁"
    pub evening: NaiveTime,
    /// "오전" 단독
    pub am: NaiveTime,
    /// 종료 시간이 없을 때 일정 길이
    pub duration_minutes: u32,
}

impl Default for TimeDefaults {
    fn default() -> Self {
        let hm = |h, m| NaiveTime::from_hms_opt(h, m, 0).expect("유효한 시각");
        Self {
            lunch: hm(12, 0),
            evening: hm(19, 0),
            am: hm(10, 0),
            duration_minutes: 60,
        }
    }
}
