//! 이벤트 조립 (PRD 4.2·4.3·5.1·5.4). LLM 추출 결과를 검토 카드에 올릴 일정 초안으로 만든다.
//!
//! 날짜·시각은 [`Resolver`]로 확정하고, 종료 기본값·종일 판정·시간대·복수 후보 플래그를 여기서
//! 붙인다. 초안은 캘린더에 쓰지 않는다 — 등록은 사용자가 카드에서 확인한 뒤에만 한다 (PRD 2절).
//! 규칙은 ADR 0003.

use chrono::{DateTime, FixedOffset, NaiveDateTime, NaiveTime, TimeDelta, TimeZone};
use chrono_tz::Tz;
use thiserror::Error;

use crate::calendar::EventTime;
use crate::context::ParseContext;
use crate::holidays::HolidayTable;
use crate::relative::{ResolveError, Resolved, Resolver};
use crate::schema::{Ambiguity, AmbiguityReason, Event, ExtractionResult};

/// 검토 카드 한 장에 해당하는 일정 초안.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EventDraft {
    /// 없으면 카드에서 입력받는다.
    pub title: Option<String>,
    pub time: EventTime,
    pub location: Option<String>,
    pub attendees: Vec<String>,
    pub recurrence: Option<String>,
    pub notes: Option<String>,
    /// 모델이 낸 플래그와 후처리가 세운 플래그를 합친 것.
    pub ambiguities: Vec<Ambiguity>,
    pub candidate_group: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum AssembleError {
    #[error("{field}: {source}")]
    Resolve {
        field: &'static str,
        source: ResolveError,
    },
    #[error("알 수 없는 시간대: {0}")]
    TimeZone(String),
    /// 서머타임 시작으로 존재하지 않는 로컬 시각.
    #[error("존재하지 않는 로컬 시각: {0}")]
    NonexistentLocalTime(NaiveDateTime),
}

/// 모델이 아니라 후처리 코드가 판단하는 플래그 (ADR 0001 7절). 모델이 내도 버린다.
const CODE_OWNED: [AmbiguityReason; 3] = [
    AmbiguityReason::YearMissing,
    AmbiguityReason::MultipleCandidates,
    AmbiguityReason::ExplicitRelativeMismatch,
];

/// 추출 결과 전체를 조립한다. 이벤트마다 결과를 따로 내므로, 한 건이 실패해도 나머지 카드는 보여 줄 수 있다.
pub fn assemble(
    result: &ExtractionResult,
    context: &ParseContext,
    holidays: &HolidayTable,
) -> Vec<Result<EventDraft, AssembleError>> {
    if result.no_event_found {
        return Vec::new();
    }
    let resolver = Resolver::new(context, holidays);
    let mut drafts: Vec<_> = result
        .events
        .iter()
        .map(|event| assemble_event(event, context, &resolver))
        .collect();
    flag_candidates(&mut drafts);
    drafts
}

fn assemble_event(
    event: &Event,
    context: &ParseContext,
    resolver: &Resolver,
) -> Result<EventDraft, AssembleError> {
    let start =
        resolver
            .resolve(&event.start, "start")
            .map_err(|source| AssembleError::Resolve {
                field: "start",
                source,
            })?;
    let end = event
        .end
        .as_ref()
        .map(|end| resolver.resolve_end(end, &start, "end"))
        .transpose()
        .map_err(|source| AssembleError::Resolve {
            field: "end",
            source,
        })?;

    let mut ambiguities: Vec<Ambiguity> = event
        .ambiguities
        .iter()
        .filter(|a| !CODE_OWNED.contains(&a.reason))
        .cloned()
        .collect();
    ambiguities.extend(start.ambiguities.iter().cloned());
    if let Some(end) = &end {
        ambiguities.extend(end.ambiguities.iter().cloned());
    }

    let time = if event.all_day || start.time.is_none() {
        all_day(&start, end.as_ref())
    } else {
        timed(&start, end.as_ref(), context, &mut ambiguities)?
    };

    Ok(EventDraft {
        title: event.title.clone(),
        time,
        location: event.location.clone(),
        attendees: event.attendees.clone(),
        recurrence: event.recurrence.clone(),
        notes: event.notes.clone(),
        ambiguities,
        candidate_group: event.candidate_group,
    })
}

/// 종일 일정. 종료일은 마지막 날(포함)이고, 없거나 시작보다 앞서면 시작일과 같다.
fn all_day(start: &Resolved, end: Option<&Resolved>) -> EventTime {
    let end = end
        .map(|e| e.date)
        .filter(|d| *d >= start.date)
        .unwrap_or(start.date);
    EventTime::AllDay {
        start: start.date,
        end,
    }
}

fn timed(
    start: &Resolved,
    end: Option<&Resolved>,
    context: &ParseContext,
    ambiguities: &mut Vec<Ambiguity>,
) -> Result<EventTime, AssembleError> {
    let start_at = start
        .date
        .and_time(start.time.expect("종일이 아닌 경우만 호출"));
    let default_end = start_at + TimeDelta::minutes(i64::from(context.defaults.duration_minutes));

    let end_at = match end {
        // 종료 시각이 없으면 기본 길이 (PRD 4.2)
        None => default_end,
        Some(e) => match e.time.map(|t| e.date.and_time(t)) {
            Some(at) if at > start_at => at,
            // "23:00~01:00"처럼 같은 날짜로 적힌 자정 넘는 종료. 12시간 이내일 때만
            Some(at)
                if e.date == start.date
                    && at + TimeDelta::days(1) - start_at <= TimeDelta::hours(12) =>
            {
                at + TimeDelta::days(1)
            }
            // 시각이 없거나 시작보다 앞서는 종료: 기본 길이로 두고 기간 불명 플래그
            _ => {
                ambiguities.push(Ambiguity {
                    field: "end".to_string(),
                    reason: AmbiguityReason::DurationUnknown,
                    alternatives: vec![format_local(
                        e.date.and_time(e.time.unwrap_or(NaiveTime::MIN)),
                        e.time.is_some(),
                    )],
                });
                default_end
            }
        },
    };

    let tz: Tz = context
        .time_zone
        .parse()
        .map_err(|_| AssembleError::TimeZone(context.time_zone.clone()))?;
    Ok(EventTime::Timed {
        start: localize(tz, start_at)?,
        end: localize(tz, end_at)?,
        time_zone: context.time_zone.clone(),
    })
}

/// 서머타임으로 같은 로컬 시각이 두 번 있으면 이른 쪽을 쓴다.
fn localize(tz: Tz, local: NaiveDateTime) -> Result<DateTime<FixedOffset>, AssembleError> {
    tz.from_local_datetime(&local)
        .earliest()
        .map(|dt| dt.fixed_offset())
        .ok_or(AssembleError::NonexistentLocalTime(local))
}

/// 같은 `candidate_group`에 두 건 이상 있으면 각 이벤트에 `multiple_candidates` 플래그와 다른 후보의
/// 시작 시각을 대안으로 붙인다 (PRD 5.4).
fn flag_candidates(drafts: &mut [Result<EventDraft, AssembleError>]) {
    let starts: Vec<(u32, usize, String)> = drafts
        .iter()
        .enumerate()
        .filter_map(|(i, d)| {
            let d = d.as_ref().ok()?;
            Some((d.candidate_group?, i, start_label(&d.time)))
        })
        .collect();
    for (group, index, _) in &starts {
        let alternatives: Vec<String> = starts
            .iter()
            .filter(|(g, i, _)| g == group && i != index)
            .map(|(_, _, label)| label.clone())
            .collect();
        if alternatives.is_empty() {
            continue;
        }
        if let Ok(draft) = &mut drafts[*index] {
            draft.ambiguities.push(Ambiguity {
                field: "start".to_string(),
                reason: AmbiguityReason::MultipleCandidates,
                alternatives,
            });
        }
    }
}

fn start_label(time: &EventTime) -> String {
    match time {
        EventTime::AllDay { start, .. } => start.to_string(),
        EventTime::Timed { start, .. } => format_local(start.naive_local(), true),
    }
}

fn format_local(at: NaiveDateTime, with_time: bool) -> String {
    if with_time {
        at.format("%Y-%m-%dT%H:%M").to_string()
    } else {
        at.date().to_string()
    }
}
