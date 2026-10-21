//! 평가셋 JSONL 형식 (PRD 5.5·5.7).
//!
//! 한 줄이 한 케이스: `{input, expected, reference_date, notes}`.
//! `expected`는 LLM 중간 표현이 아니라 후처리까지 끝난 확정값이다 — 정확도는 최종 날짜·시간으로 잰다.

use std::fmt;
use std::path::Path;

use chrono::{NaiveDate, NaiveDateTime};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EvalCase {
    /// 붙여 넣는 메시지 원문.
    pub input: String,
    pub expected: Expected,
    /// 상대 날짜 해석의 기준일 ("오늘").
    pub reference_date: NaiveDate,
    #[serde(default)]
    pub notes: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Expected {
    #[serde(default)]
    pub events: Vec<ExpectedEvent>,
    #[serde(default)]
    pub no_event_found: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExpectedEvent {
    #[serde(default)]
    pub title: Option<String>,
    pub start: LocalTime,
    #[serde(default)]
    pub end: Option<LocalTime>,
    #[serde(default)]
    pub all_day: bool,
    #[serde(default)]
    pub location: Option<String>,
    /// RFC 5545 RRULE 문자열.
    #[serde(default)]
    pub recurrence: Option<String>,
}

/// 로컬 날짜(`2026-10-24`, 종일) 또는 로컬 일시(`2026-10-24T10:00`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LocalTime {
    Date(NaiveDate),
    DateTime(NaiveDateTime),
}

const DATE_FORMAT: &str = "%Y-%m-%d";
const DATETIME_FORMAT: &str = "%Y-%m-%dT%H:%M";

impl fmt::Display for LocalTime {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Date(d) => write!(f, "{}", d.format(DATE_FORMAT)),
            Self::DateTime(dt) => write!(f, "{}", dt.format(DATETIME_FORMAT)),
        }
    }
}

impl Serialize for LocalTime {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for LocalTime {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let raw = String::deserialize(d)?;
        if let Ok(dt) = NaiveDateTime::parse_from_str(&raw, DATETIME_FORMAT) {
            return Ok(Self::DateTime(dt));
        }
        NaiveDate::parse_from_str(&raw, DATE_FORMAT)
            .map(Self::Date)
            .map_err(|_| {
                serde::de::Error::custom(format!(
                    "YYYY-MM-DD 또는 YYYY-MM-DDTHH:MM 형식이어야 한다: {raw}"
                ))
            })
    }
}

/// 형식 오류. 줄 번호는 1부터.
#[derive(Debug)]
pub struct LineError {
    pub line: usize,
    pub message: String,
}

/// JSONL 문자열을 읽는다. 빈 줄은 건너뛰고, 형식 오류는 모두 모아서 돌려준다.
pub fn parse_set(content: &str) -> Result<Vec<EvalCase>, Vec<LineError>> {
    let mut cases = Vec::new();
    let mut errors = Vec::new();
    for (index, line) in content.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        match serde_json::from_str::<EvalCase>(line) {
            Ok(case) => cases.push(case),
            Err(e) => errors.push(LineError {
                line: index + 1,
                message: e.to_string(),
            }),
        }
    }
    if errors.is_empty() {
        Ok(cases)
    } else {
        Err(errors)
    }
}

pub fn load_set(path: &Path) -> Result<Vec<EvalCase>, Vec<LineError>> {
    let content = std::fs::read_to_string(path).map_err(|e| {
        vec![LineError {
            line: 0,
            message: format!("{}: {e}", path.display()),
        }]
    })?;
    parse_set(&content)
}
