//! 공휴일 테이블 (PRD 5.3).
//!
//! 출처는 한국천문연구원 특일 정보 API이고, `scripts/update_holidays.py`가 `core/data/holidays.json`을
//! 만든다. 앱은 런타임에 API를 부르지 않고 이 내장 테이블만 쓴다 (ADR 0001 8절).

use chrono::{Days, NaiveDate};
use serde::{Deserialize, Serialize};

const BUNDLED: &str = include_str!("../data/holidays.json");

/// 공휴일 하루. `name`은 API의 `dateName` 값 그대로다.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Holiday {
    pub date: NaiveDate,
    pub name: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct HolidayTable {
    /// 날짜 오름차순.
    entries: Vec<Holiday>,
}

impl HolidayTable {
    pub fn new(mut entries: Vec<Holiday>) -> Self {
        entries.sort_by_key(|h| h.date);
        Self { entries }
    }

    /// 앱에 내장된 테이블.
    pub fn bundled() -> Self {
        let entries = serde_json::from_str(BUNDLED).expect("core/data/holidays.json 형식 오류");
        Self::new(entries)
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// `name`이 이어지는 날짜 묶음(연휴) 중 `from` 이후에 끝나는 첫 묶음의 첫날.
    pub fn block_start(&self, name: &str, from: NaiveDate) -> Option<NaiveDate> {
        let mut start: Option<NaiveDate> = None;
        let mut prev: Option<NaiveDate> = None;
        for h in self.entries.iter().filter(|h| h.name == name) {
            let continues = prev.and_then(|p| p.checked_add_days(Days::new(1))) == Some(h.date);
            if !continues {
                if let (Some(s), Some(p)) = (start, prev) {
                    if p >= from {
                        return Some(s);
                    }
                }
                start = Some(h.date);
            }
            prev = Some(h.date);
        }
        match (start, prev) {
            (Some(s), Some(p)) if p >= from => Some(s),
            _ => None,
        }
    }
}
