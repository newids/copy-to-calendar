//! 채점 (PRD 2절 "파싱 정확도 (날짜·시간)"). 모델 출력을 core로 조립한 결과를 `expected`와 비교한다.
//!
//! 케이스 하나는 모든 일정의 시작·종료·종일 여부·후보 묶음이 맞아야 정답이다. 제목·장소는 아직 채점하지
//! 않는다 (ADR 0003 4절).

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
    let mut actual = Vec::with_capacity(drafts.len());
    for (i, draft) in drafts.into_iter().enumerate() {
        let draft = draft.map_err(|e| format!("#{i}: {e}"))?;
        check_event(&expected.events[i], &draft).map_err(|e| format!("#{i}: {e}"))?;
        actual.push(draft);
    }
    check_groups(&expected.events, &actual)
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
fn check_groups(expected: &[ExpectedEvent], actual: &[EventDraft]) -> Result<(), String> {
    for i in 0..expected.len() {
        for j in i..expected.len() {
            let same = |a: Option<u32>, b: Option<u32>| a.is_some() && a == b;
            let want = if i == j {
                expected[i].candidate_group.is_some()
            } else {
                same(expected[i].candidate_group, expected[j].candidate_group)
            };
            let got = if i == j {
                actual[i].candidate_group.is_some()
            } else {
                same(actual[i].candidate_group, actual[j].candidate_group)
            };
            if want != got {
                return Err(format!("#{i}·#{j} 후보 묶음이 기대와 다름"));
            }
        }
    }
    Ok(())
}
