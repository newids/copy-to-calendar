//! 캘린더에 쓰는 확정된 일정과 식별자 (PRD 7절).
//!
//! [`crate::schema::Event`]는 LLM이 낸 중간 표현이고, [`CalendarEvent`]는 후처리와 사용자 확인을
//! 거친 최종값이다.

use chrono::{DateTime, FixedOffset, NaiveDate};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CalendarId(pub String);

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct EventId(pub String);

/// 캘린더에 있는 이벤트를 가리키는 참조. Google API는 수정·삭제에 캘린더 ID가 함께 필요하다.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct EventRef {
    pub calendar: CalendarId,
    pub event: EventId,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Calendar {
    pub id: CalendarId,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CalendarEvent {
    pub title: String,
    pub time: EventTime,
    pub location: Option<String>,
    pub attendees: Vec<String>,
    /// RFC 5545 RRULE 문자열.
    pub recurrence: Option<String>,
    /// 원문 포함 여부는 사용자 옵션 (PRD 4.3).
    pub notes: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum EventTime {
    /// 종일 일정. `end`는 마지막 날(포함). 배타적 종료일이 필요한 API는 어댑터가 변환한다.
    AllDay { start: NaiveDate, end: NaiveDate },
    /// `time_zone`은 IANA 이름. 반복 일정 전개와 Google API `timeZone`에 쓴다.
    Timed {
        start: DateTime<FixedOffset>,
        end: DateTime<FixedOffset>,
        time_zone: String,
    },
}

/// 캘린더에서 조회한 기존 이벤트 (중복·충돌 판정 결과).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StoredEvent {
    pub reference: EventRef,
    pub event: CalendarEvent,
}
