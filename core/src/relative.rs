//! 날짜·시간 확정 (PRD 5.1·5.3). 모델이 계산한 날짜를 믿지 않고, `relative` 중간 표현을 여기서
//! 결정적으로 확정한다. 같은 입력과 컨텍스트면 항상 같은 결과가 나온다.
//!
//! 규칙과 그 근거(PRD에 없어 이 모듈에서 정한 해석 포함)는 ADR 0002에 있다.

use chrono::{Datelike, Days, Months, NaiveDate, NaiveTime, Timelike};
use thiserror::Error;

use crate::context::ParseContext;
use crate::holidays::HolidayTable;
use crate::schema::{
    Ambiguity, AmbiguityReason, DateTimeField, HolidayRef, IsoWeekday, MonthPeriod, RelativeExpr,
    TimeWord,
};

/// 확정된 로컬 날짜·시각. 시간대는 아직 붙이지 않는다.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Resolved {
    pub date: NaiveDate,
    /// 시각 표현이 전혀 없으면 None (종일 후보).
    pub time: Option<NaiveTime>,
    /// 후처리가 세운 모호성 플래그. 모델이 낸 `Event.ambiguities`와 합쳐 카드에 쓴다.
    pub ambiguities: Vec<Ambiguity>,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ResolveError {
    #[error("explicit과 relative가 모두 비어 있음")]
    Empty,
    #[error("해석할 수 없는 값: {0}")]
    Invalid(String),
    #[error("공휴일 테이블에 없음: {0}")]
    UnknownHoliday(String),
}

pub struct Resolver<'a> {
    context: &'a ParseContext,
    holidays: &'a HolidayTable,
}

impl<'a> Resolver<'a> {
    pub fn new(context: &'a ParseContext, holidays: &'a HolidayTable) -> Self {
        Self { context, holidays }
    }

    fn today(&self) -> NaiveDate {
        self.context.reference.date()
    }

    /// 시작 시각처럼 기준일에서 출발하는 필드를 확정한다. `path`는 플래그의 필드 경로 접두사(`"start"`).
    pub fn resolve(&self, field: &DateTimeField, path: &str) -> Result<Resolved, ResolveError> {
        self.resolve_with_base(field, path, None)
    }

    /// 종료 시각을 확정한다. 날짜 표현이 없으면 시작일을 쓰고, 오전·오후가 없으면 시작 이후 가장
    /// 가까운 시각을 고른다 ("오후 2시~4시" → 16:00).
    pub fn resolve_end(
        &self,
        end: &DateTimeField,
        start: &Resolved,
        path: &str,
    ) -> Result<Resolved, ResolveError> {
        self.resolve_with_base(end, path, Some(start))
    }

    fn resolve_with_base(
        &self,
        field: &DateTimeField,
        path: &str,
        start: Option<&Resolved>,
    ) -> Result<Resolved, ResolveError> {
        let explicit = field.explicit.as_ref().map(|e| Resolved {
            date: e.date,
            time: e.time,
            ambiguities: Vec::new(),
        });
        let relative = field
            .relative
            .as_ref()
            .map(|r| self.relative(r, path, start))
            .transpose()?;

        match (explicit, relative) {
            (Some(mut e), Some(r)) => {
                // 원문에 적힌 절대 날짜를 따르고, 상대 표현 계산값을 대안으로 남긴다 (PRD 5.1).
                if (e.date, e.time) != (r.date, r.time) {
                    e.ambiguities.push(Ambiguity {
                        field: path.to_string(),
                        reason: AmbiguityReason::ExplicitRelativeMismatch,
                        alternatives: vec![format_date_time(r.date, r.time)],
                    });
                }
                Ok(e)
            }
            (Some(e), None) => Ok(e),
            (None, Some(r)) => Ok(r),
            (None, None) => Err(ResolveError::Empty),
        }
    }

    fn relative(
        &self,
        r: &RelativeExpr,
        path: &str,
        start: Option<&Resolved>,
    ) -> Result<Resolved, ResolveError> {
        let mut ambiguities = Vec::new();
        let today = self.today();

        let anchor = if let Some(holiday) = &r.holiday {
            Some(self.holiday_date(holiday)?)
        } else if r.month.is_some()
            || r.day.is_some()
            || r.month_offset.is_some()
            || r.month_period.is_some()
        {
            Some(self.calendar_date(r, path, &mut ambiguities)?)
        } else if r.week_offset.is_some() || r.weekday.is_some() {
            Some(self.week_date(r.week_offset, r.weekday))
        } else {
            None
        };
        let has_date = anchor.is_some() || r.day_offset.is_some();
        let base = match (anchor, start) {
            (Some(d), _) => d,
            (None, Some(s)) if r.day_offset.is_none() => s.date,
            _ => today,
        };
        let mut date = add_days(base, r.day_offset.unwrap_or(0))?;

        // 날짜 표현 없이 시각만 있는 종료 필드는 시작 시각 이후로 맞춘다.
        let after = start.filter(|_| !has_date).and_then(|s| s.time);
        let (time, next_day) = self.time(r, path, after, &mut ambiguities)?;
        if next_day {
            date = add_days(date, 1)?;
        }

        Ok(Resolved {
            date,
            time,
            ambiguities,
        })
    }

    /// 주 단위 표현: "이번 주 금", "담주 목", "다담주", "금요일에".
    fn week_date(&self, week_offset: Option<i32>, weekday: Option<IsoWeekday>) -> NaiveDate {
        let today = self.today();
        let rules = self.context.week;
        let start = i64::from(rules.start.number());
        let current = i64::from(today.weekday().number_from_monday());
        let week_start = today - chrono::Duration::days((current - start).rem_euclid(7));

        match (week_offset, weekday) {
            (Some(offset), Some(w)) => {
                let into_week = (i64::from(w.number()) - start).rem_euclid(7);
                let mut date =
                    week_start + chrono::Duration::days(7 * i64::from(offset) + into_week);
                // PRD 5.3: 기준일이 토·일이면 "이번 주 X"를 다음 주로 넘긴다. 이미 지난 날짜일 때만.
                if offset == 0
                    && rules.weekend_rolls_to_next_week
                    && is_weekend(today)
                    && date < today
                {
                    date += chrono::Duration::days(7);
                }
                date
            }
            (Some(offset), None) => {
                let date = week_start + chrono::Duration::days(7 * i64::from(offset));
                date.max(today)
            }
            (None, Some(w)) => {
                let ahead = (i64::from(w.number()) - current).rem_euclid(7);
                today + chrono::Duration::days(ahead)
            }
            (None, None) => today,
        }
    }

    /// 월·일 표현: "24일", "10/24", "다음 달 초", "11월 하순".
    fn calendar_date(
        &self,
        r: &RelativeExpr,
        path: &str,
        ambiguities: &mut Vec<Ambiguity>,
    ) -> Result<NaiveDate, ResolveError> {
        let today = self.today();
        let day = match (r.day, r.month_period) {
            (Some(d), _) => d,
            (None, Some(p)) => period_first_day(p),
            (None, None) => 1,
        };

        // 일이 없는 표현("11월", "하순")은 범위다. 범위의 마지막 날.
        let range_end = |d: NaiveDate| match (r.day, r.month_period) {
            (Some(_), _) => d,
            (None, Some(p)) => period_last_day(d, p),
            (None, None) => month_end(d),
        };

        let date = if let Some(month) = r.month {
            let candidate = ymd(today.year(), month, day)?;
            self.apply_year_rule(candidate, range_end(candidate), path, ambiguities)?
        } else if let Some(offset) = r.month_offset {
            let first = month_start(today);
            let target = shift_months(first, offset)?;
            ymd(target.year(), target.month(), day)?
        } else if r.day.is_some() {
            // "24일": 기준일 이후 가장 가까운 24일. 그 달에 없는 날(31일)이면 있는 달까지 넘긴다.
            let first = month_start(today);
            (0..=12)
                .filter_map(|i| shift_months(first, i).ok())
                .filter_map(|m| NaiveDate::from_ymd_opt(m.year(), m.month(), day))
                .find(|d| *d >= today)
                .ok_or_else(|| ResolveError::Invalid(format!("day {day}")))?
        } else {
            // 월 구간만: 이번 달 구간이 다 지났으면 다음 달.
            let this_month = ymd(today.year(), today.month(), day)?;
            if range_end(this_month) < today {
                let next = shift_months(month_start(today), 1)?;
                ymd(next.year(), next.month(), day)?
            } else {
                this_month
            }
        };

        // 범위 안에 기준일이 있으면 남은 날 중 가장 빠른 날 = 기준일.
        Ok(if date <= today && today <= range_end(date) {
            today
        } else {
            date
        })
    }

    /// PRD 5.3 연도 누락 규칙: 기준일 이후면 올해, 과거 6개월 이내면 올해, 넘으면 내년.
    /// 지난 날짜로 해석될 때는 `year_missing` 플래그와 다른 연도 대안을 남긴다.
    /// `last`는 범위 표현("10월 하순")의 마지막 날로, 아직 끝나지 않은 범위는 지난 것으로 보지 않는다.
    fn apply_year_rule(
        &self,
        candidate: NaiveDate,
        last: NaiveDate,
        path: &str,
        ambiguities: &mut Vec<Ambiguity>,
    ) -> Result<NaiveDate, ResolveError> {
        let today = self.today();
        if last >= today {
            return Ok(candidate);
        }
        let next_year = candidate
            .with_year(candidate.year() + 1)
            .ok_or_else(|| ResolveError::Invalid(format!("{candidate} 다음 해")))?;
        let six_months_ago = today
            .checked_sub_months(Months::new(6))
            .ok_or_else(|| ResolveError::Invalid(format!("{today} - 6개월")))?;
        let (chosen, other) = if candidate >= six_months_ago {
            (candidate, next_year)
        } else {
            (next_year, candidate)
        };
        ambiguities.push(Ambiguity {
            field: format!("{path}.date"),
            reason: AmbiguityReason::YearMissing,
            alternatives: vec![other.to_string()],
        });
        Ok(chosen)
    }

    fn holiday_date(&self, holiday: &HolidayRef) -> Result<NaiveDate, ResolveError> {
        let start = self
            .holidays
            .block_start(&holiday.name, self.today())
            .ok_or_else(|| ResolveError::UnknownHoliday(holiday.name.clone()))?;
        add_days(start, holiday.day_offset)
    }

    /// 시각 확정. 반환값의 bool은 다음 날로 넘어가는지("밤 12시").
    fn time(
        &self,
        r: &RelativeExpr,
        path: &str,
        after: Option<NaiveTime>,
        ambiguities: &mut Vec<Ambiguity>,
    ) -> Result<(Option<NaiveTime>, bool), ResolveError> {
        let minute = r.minute.unwrap_or(0);
        if minute > 59 {
            return Err(ResolveError::Invalid(format!("minute {minute}")));
        }
        let Some(hour) = r.hour else {
            return Ok((
                r.time_word.map(|w| self.context.defaults.time_for(w)),
                false,
            ));
        };
        if hour > 23 {
            return Err(ResolveError::Invalid(format!("hour {hour}")));
        }

        let (hour24, next_day) = match r.time_word {
            _ if hour == 0 || hour >= 13 => (hour, false),
            Some(word) => word_hour(word, hour),
            None => {
                if let Some(h) = after.and_then(|a| following_hour(hour, minute, a)) {
                    (h, false)
                } else {
                    let (h, alternative) = self.guess_am_pm(hour);
                    if let Some(alt) = alternative {
                        ambiguities.push(Ambiguity {
                            field: format!("{path}.time"),
                            reason: AmbiguityReason::AmPmUnknown,
                            alternatives: vec![format_time(hm(alt, minute)?)],
                        });
                    }
                    (h, false)
                }
            }
        };
        Ok((Some(hm(hour24, minute)?), next_day))
    }

    /// 오전·오후 표시 없는 1~12시 (PRD 5.3). 반환: (선택한 시, 대안 시).
    fn guess_am_pm(&self, hour: u32) -> (u32, Option<u32>) {
        match hour {
            12 => (12, None),
            // "1~6시 단독은 오후로 추정"
            1..=6 => (hour + 12, Some(hour)),
            // 7~11시: 근무 시작 전 시각이면 오후, 아니면 오전.
            _ => {
                if hour < self.context.working_hours.start.hour() {
                    (hour + 12, Some(hour))
                } else {
                    (hour, Some(hour + 12))
                }
            }
        }
    }
}

/// 시간대 말과 함께 쓴 1~12시를 24시간제로. 반환의 bool은 다음 날로 넘어가는지.
fn word_hour(word: TimeWord, hour: u32) -> (u32, bool) {
    match word {
        TimeWord::Dawn | TimeWord::Am => (hour % 12, false),
        TimeWord::Morning => (hour, false),
        TimeWord::Lunch => (if hour <= 5 { hour + 12 } else { hour }, false),
        TimeWord::Pm | TimeWord::Evening => (if hour < 12 { hour + 12 } else { hour }, false),
        // "밤 12시" = 다음 날 0시, "밤 1~5시" = 다음 날 새벽
        TimeWord::Night => match hour {
            12 => (0, true),
            1..=5 => (hour, true),
            _ => (hour + 12, false),
        },
    }
}

/// 종료 시각: `hour`와 `hour + 12` 중 `after`보다 늦은 가장 이른 시각.
fn following_hour(hour: u32, minute: u32, after: NaiveTime) -> Option<u32> {
    [hour % 12, hour % 12 + 12]
        .into_iter()
        .find(|h| hm(*h, minute).is_ok_and(|t| t > after))
}

fn period_first_day(period: MonthPeriod) -> u32 {
    match period {
        MonthPeriod::Start | MonthPeriod::Early => 1,
        MonthPeriod::Middle => 11,
        MonthPeriod::Late | MonthPeriod::End => 21,
    }
}

fn period_last_day(date: NaiveDate, period: MonthPeriod) -> NaiveDate {
    match period {
        MonthPeriod::Start | MonthPeriod::Early => date.with_day(10),
        MonthPeriod::Middle => date.with_day(20),
        MonthPeriod::Late | MonthPeriod::End => None,
    }
    .unwrap_or_else(|| month_end(date))
}

fn month_end(date: NaiveDate) -> NaiveDate {
    shift_months(month_start(date), 1)
        .ok()
        .and_then(|d| d.pred_opt())
        .unwrap_or(date)
}

fn is_weekend(date: NaiveDate) -> bool {
    date.weekday().number_from_monday() >= 6
}

fn month_start(date: NaiveDate) -> NaiveDate {
    date.with_day(1).expect("1일은 항상 있다")
}

fn shift_months(date: NaiveDate, offset: i32) -> Result<NaiveDate, ResolveError> {
    let months = Months::new(offset.unsigned_abs());
    if offset >= 0 {
        date.checked_add_months(months)
    } else {
        date.checked_sub_months(months)
    }
    .ok_or_else(|| ResolveError::Invalid(format!("month_offset {offset}")))
}

fn add_days(date: NaiveDate, days: i32) -> Result<NaiveDate, ResolveError> {
    let n = Days::new(u64::from(days.unsigned_abs()));
    if days >= 0 {
        date.checked_add_days(n)
    } else {
        date.checked_sub_days(n)
    }
    .ok_or_else(|| ResolveError::Invalid(format!("day_offset {days}")))
}

fn ymd(year: i32, month: u32, day: u32) -> Result<NaiveDate, ResolveError> {
    NaiveDate::from_ymd_opt(year, month, day)
        .ok_or_else(|| ResolveError::Invalid(format!("{year}-{month}-{day}")))
}

fn hm(hour: u32, minute: u32) -> Result<NaiveTime, ResolveError> {
    NaiveTime::from_hms_opt(hour, minute, 0)
        .ok_or_else(|| ResolveError::Invalid(format!("{hour}:{minute}")))
}

fn format_time(time: NaiveTime) -> String {
    time.format("%H:%M").to_string()
}

fn format_date_time(date: NaiveDate, time: Option<NaiveTime>) -> String {
    match time {
        Some(t) => format!("{}T{}", date, format_time(t)),
        None => date.to_string(),
    }
}
