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

    /// 테이블에 있는 공휴일 이름 (중복 없이, 가나다순). 대체공휴일은 뺀다 — 메시지에서 이름으로 부르지 않는다.
    pub fn names(&self) -> Vec<&str> {
        let mut names: Vec<&str> = self
            .entries
            .iter()
            .map(|h| h.name.as_str())
            .filter(|n| !n.starts_with("대체공휴일"))
            .collect();
        names.sort_unstable();
        names.dedup();
        names
    }

    /// `name`이 이어지는 날짜 묶음(연휴) 중 `from` 이후에 끝나는 첫 묶음의 (첫날, 마지막 날).
    /// 특일 정보 API는 설날·추석 연휴 3일을 모두 같은 이름으로 준다.
    pub fn block(&self, name: &str, from: NaiveDate) -> Option<(NaiveDate, NaiveDate)> {
        let mut block: Option<(NaiveDate, NaiveDate)> = None;
        for h in self.entries.iter().filter(|h| h.name == name) {
            match block {
                Some((start, end)) if end.checked_add_days(Days::new(1)) == Some(h.date) => {
                    block = Some((start, h.date));
                }
                Some((_, end)) if end >= from => return block,
                _ => block = Some((h.date, h.date)),
            }
        }
        block.filter(|(_, end)| *end >= from)
    }
}
