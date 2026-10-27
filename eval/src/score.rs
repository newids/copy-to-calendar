//! 채점 (PRD 2절 "파싱 정확도 (날짜·시간)"). 모델 출력을 core로 조립한 결과를 `expected`와 비교한다.
//!
//! 케이스 하나는 모든 일정의 시작·종료·종일 여부·후보 묶음이 맞아야 정답이다. 일정 순서는 보지 않고,
//! 제목·장소는 채점하지 않는다 (ADR 0003 4절).

use c2c_core::assemble::{EventDraft, assemble};
use c2c_core::calendar::EventTime;
use c2c_core::context::ParseContext;
use c2c_core::holidays::HolidayTable;
use c2c_core::schema::ExtractionResult;

use crate::{EvalCase, ExpectedEvent, LocalTime};

/// 평가셋은 한국어 메시지 기준이라 시간대를 고정한다. 해석기는 기준일의 날짜만 쓴다.
const EVAL_TIME_ZONE: &str = "Asia/Seoul";

pub fn context_for(case: &EvalCase) -> ParseContext {
    ParseContext {
        reference: case.reference_date.and_time(chrono::NaiveTime::MIN),
        time_zone: EVAL_TIME_ZONE.to_string(),
        working_hours: Default::default(),
        defaults: Default::default(),
        week: Default::default(),
        recent_locations: Vec::new(),
    }
}

/// 맞으면 Ok, 틀리면 이유.
pub fn score_case(
    case: &EvalCase,
    output: &ExtractionResult,
    holidays: &HolidayTable,
) -> Result<(), String> {
    let drafts = assemble(output, &context_for(case), holidays);
    let expected = &case.expected;
    if expected.no_event_found {
        return if drafts.is_empty() {
            Ok(())
        } else {
            Err(format!("일정 없음이어야 하는데 {}건", drafts.len()))
        };
    }
    if drafts.len() != expected.events.len() {
        return Err(format!(
            "일정 {}건이어야 하는데 {}건",
            expected.events.len(),
            drafts.len()
        ));
    }
    let actual = drafts
        .into_iter()
        .enumerate()
        .map(|(i, d)| d.map_err(|e| format!("출력 #{i}: {e}")))
        .collect::<Result<Vec<_>, _>>()?;

    // 순서와 무관하게 기대 일정마다 맞는 출력 일정을 하나씩 짝짓는다 (ADR 0003 4절).
    let mut pairs = vec![0; actual.len()];
    let mut used = vec![false; actual.len()];
    if pair_up(&expected.events, &actual, 0, &mut pairs, &mut used) {
        return Ok(());
    }
    Err(explain(&expected.events, &actual))
}

/// 백트래킹으로 짝을 찾는다. `pairs[i]`는 기대 일정 i에 짝지은 출력 일정 번호.
fn pair_up(
    expected: &[ExpectedEvent],
    actual: &[EventDraft],
    i: usize,
    pairs: &mut [usize],
    used: &mut [bool],
) -> bool {
    if i == expected.len() {
        return groups_match(expected, actual, pairs);
    }
    for j in 0..actual.len() {
        if used[j] || check_event(&expected[i], &actual[j]).is_err() {
            continue;
        }
        used[j] = true;
        pairs[i] = j;
        if pair_up(expected, actual, i + 1, pairs, used) {
            return true;
        }
        used[j] = false;
    }
    false
}

/// 짝을 찾지 못한 이유. 맞는 출력이 하나도 없는 첫 기대 일정을 알려 준다.
fn explain(expected: &[ExpectedEvent], actual: &[EventDraft]) -> String {
    for (i, e) in expected.iter().enumerate() {
        if actual.iter().all(|a| check_event(e, a).is_err()) {
            let nearest = actual
                .get(i)
                .and_then(|a| check_event(e, a).err())
                .map(|reason| format!(" (같은 순서의 출력: {reason})"))
                .unwrap_or_default();
            return format!("기대 #{i} (시작 {})와 맞는 일정 없음{nearest}", e.start);
        }
    }
    "후보 묶음이 기대와 다름".to_string()
}

fn check_event(expected: &ExpectedEvent, draft: &EventDraft) -> Result<(), String> {
    let (start, end) = match &draft.time {
        EventTime::AllDay { start, end } => (LocalTime::Date(*start), LocalTime::Date(*end)),
        EventTime::Timed { start, end, .. } => (
            LocalTime::DateTime(start.naive_local()),
            LocalTime::DateTime(end.naive_local()),
        ),
    };
    if start != expected.start {
        return Err(format!("시작 {start}, 기대 {}", expected.start));
    }
    if let Some(expected_end) = expected.end {
        if end != expected_end {
            return Err(format!("종료 {end}, 기대 {expected_end}"));
        }
    }
    let all_day = matches!(draft.time, EventTime::AllDay { .. });
    if all_day != expected.all_day {
        return Err(format!("종일 {all_day}, 기대 {}", expected.all_day));
    }
    Ok(())
}

/// 후보 묶음은 번호가 아니라 "어느 일정끼리 같은 묶음인가"로 비교한다.
fn groups_match(expected: &[ExpectedEvent], actual: &[EventDraft], pairs: &[usize]) -> bool {
    let same = |a: Option<u32>, b: Option<u32>| a.is_some() && a == b;
    (0..expected.len()).all(|i| {
        let a_i = actual[pairs[i]].candidate_group;
        expected[i].candidate_group.is_some() == a_i.is_some()
            && (i + 1..expected.len()).all(|j| {
                same(expected[i].candidate_group, expected[j].candidate_group)
                    == same(a_i, actual[pairs[j]].candidate_group)
            })
    })
}
