//! LLM 프롬프트 (PRD 5.2). 모든 공급자가 같은 프롬프트와 같은 출력 스키마를 쓴다.
//!
//! `instructions`는 앱 버전마다 고정이라 프롬프트 캐시에 올리고, 요청마다 바뀌는 `context`는 뒤에 둔다.

use std::sync::OnceLock;

use chrono::Datelike;
use serde::Serialize;

use crate::context::ParseContext;
use crate::holidays::HolidayTable;

/// PRD 5.1 출력 스키마 (JSON Schema, 구조화 출력 strict 모드 형식).
pub const EXTRACTION_SCHEMA: &str = include_str!("extraction.schema.json");

pub fn extraction_schema() -> serde_json::Value {
    serde_json::from_str(EXTRACTION_SCHEMA).expect("extraction.schema.json 형식 오류")
}

/// 요청 1건의 프롬프트. 클라우드 프록시에는 세 부분을 그대로 보내고, 로컬 서버에는
/// `instructions + context`를 system 메시지로, `text`를 user 메시지로 보낸다.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Prompt {
    pub instructions: String,
    pub context: String,
    pub text: String,
}

impl Prompt {
    pub fn new(text: &str, context: &ParseContext) -> Self {
        Self {
            instructions: instructions().to_string(),
            context: render_context(context),
            text: text.to_string(),
        }
    }

    pub fn system(&self) -> String {
        format!("{}\n\n{}", self.instructions, self.context)
    }
}

const WEEKDAYS: [&str; 7] = ["월", "화", "수", "목", "금", "토", "일"];

fn render_context(c: &ParseContext) -> String {
    let reference = c.reference;
    let weekday = WEEKDAYS[reference.weekday().num_days_from_monday() as usize];
    let hm = |t: chrono::NaiveTime| t.format("%H:%M").to_string();
    let d = &c.defaults;
    let mut lines = vec![
        "# 이번 요청의 컨텍스트".to_string(),
        format!(
            "- 기준 일시(오늘): {} ({weekday}) {}",
            reference.format("%Y-%m-%d"),
            reference.format("%H:%M")
        ),
        format!("- 시간대: {}", c.time_zone),
        format!(
            "- 근무시간: {}~{}",
            hm(c.working_hours.start),
            hm(c.working_hours.end)
        ),
        format!(
            "- 기본 시각: 새벽 {}, 아침 {}, 오전 {}, 점심 {}, 오후 {}, 저녁 {}, 밤 {}",
            hm(d.dawn),
            hm(d.morning),
            hm(d.am),
            hm(d.lunch),
            hm(d.pm),
            hm(d.evening),
            hm(d.night)
        ),
    ];
    if !c.recent_locations.is_empty() {
        let recent: Vec<&str> = c
            .recent_locations
            .iter()
            .take(10)
            .map(String::as_str)
            .collect();
        lines.push(format!("- 최근 사용한 장소: {}", recent.join(", ")));
    }
    lines.join("\n")
}

fn instructions() -> &'static str {
    static TEXT: OnceLock<String> = OnceLock::new();
    TEXT.get_or_init(|| {
        let holidays = HolidayTable::bundled();
        let names = holidays.names().join(", ");
        format!("{INSTRUCTIONS}\n\n## 공휴일 이름\n`holiday.name`에는 다음 이름 중 하나만 쓴다: {names}\n\n{EXAMPLES}")
    })
}

const INSTRUCTIONS: &str = r#"너는 메시지에서 일정을 뽑아 JSON으로 내는 추출기다. 카톡·문자·슬랙·메일로 받은 문구가 들어온다.

## 가장 중요한 규칙: 날짜를 계산하지 마라
- 원문에 연도까지 적힌 절대 날짜("2026.10.24", "2026년 10월 24일")만 `explicit`에 넣는다
- 그 밖의 모든 날짜·시각은 `explicit`을 null로 두고, 원문 표현을 `relative` 필드로 옮기기만 한다. 날짜 계산은 앱이 한다
- "10/24", "24일"처럼 연도가 없으면 `relative`의 `month`·`day`로 낸다. 연도를 붙이지 마라

## relative 필드
- `day_offset`: 오늘 0, 내일 1, 모레 2, 글피 3
- `week_offset`: 이번 주 0, 다음 주·담주 1, 다다음 주·다담주 2
- `weekday`: 1=월, 2=화, 3=수, 4=목, 5=금, 6=토, 7=일
- `month_offset`: 이번 달 0, 다음 달 1
- `month`, `day`: 원문에 적힌 월·일 그대로
- `month_period`: "월초", "상순", "중순", "하순", "월말" 중 하나
- `holiday`: 명절·공휴일 기준 표현. `name`은 아래 공휴일 이름 목록에서 고르고, `day_offset`은 당일 기준 일수("설 전날"은 -1), "추석 연휴"처럼 연휴 전체를 가리키면 `whole_period`를 true
- `time_word`: "새벽", "아침", "오전", "점심", "오후", "저녁", "밤" 중 원문에 있는 것
- `hour`, `minute`: 원문에 적힌 숫자 그대로. "저녁 7시"는 `time_word` "저녁", `hour` 7이다. 19로 바꾸지 마라. "7시 반"은 `minute` 30
- 쓰지 않는 필드는 null

## 일정 필드
- `title`: 원문 언어 그대로. 원문에 일정 이름이 없으면 null. 지어내지 마라
- `start`는 필수. `end`는 원문에 끝 시각이나 기간이 있을 때만, 없으면 null
- `all_day`: 시각 없이 날짜만 있는 행사면 true
- `location`, `attendees`: 원문에 있는 것만. 없으면 null, []
- `recurrence`: "매주 월"처럼 반복이면 RFC 5545 RRULE(`FREQ=WEEKLY;BYDAY=MO`), 아니면 null
- `notes`: 일정과 관련된 원문 문장 발췌
- `confidence`: 0~1. 해석에 자신 없을수록 낮게
- `ambiguities`: 오전·오후를 알 수 없으면 `am_pm_unknown`, 기간을 알 수 없으면 `duration_unknown`. 그 밖의 판단은 앱이 한다
- 날짜 후보가 여럿이면("이번 주 금요일 or 다음 주 화요일") 후보마다 일정을 따로 내고 같은 `candidate_group` 번호를 준다. 일정 두 개인지 후보 두 개인지 확신이 없어도 따로 낸다. 아니면 null
- 한 메시지에 일정이 여러 개면 `events`에 여러 개를 낸다
- 일정이 없으면 `events`는 []이고 `no_event_found`는 true"#;

const EXAMPLES: &str = r#"## 예시 (기준일 2026-10-19 월요일, 쓰지 않는 필드 생략)
입력: 담주 목욜 저녁 7시 강남역 2번출구 앞에서 봐요
출력: {"events":[{"title":null,"start":{"explicit":null,"relative":{"week_offset":1,"weekday":4,"time_word":"저녁","hour":7},"confidence":0.9},"end":null,"all_day":false,"location":"강남역 2번 출구","notes":"담주 목욜 저녁 7시 강남역 2번출구 앞에서 봐요","candidate_group":null}],"no_event_found":false}

입력: [공지] 10/24(토) 오전 10시 ~ 12시 학부모 설명회, 3층 강당
출력: {"events":[{"title":"학부모 설명회","start":{"explicit":null,"relative":{"month":10,"day":24,"time_word":"오전","hour":10},"confidence":0.95},"end":{"explicit":null,"relative":{"hour":12},"confidence":0.95},"all_day":false,"location":"3층 강당","candidate_group":null}],"no_event_found":false}

입력: 매주 월 오전 9시 스탠드업 30분
출력: {"events":[{"title":"스탠드업","start":{"explicit":null,"relative":{"weekday":1,"time_word":"오전","hour":9},"confidence":0.9},"end":{"explicit":null,"relative":{"hour":9,"minute":30},"confidence":0.9},"all_day":false,"recurrence":"FREQ=WEEKLY;BYDAY=MO","candidate_group":null}],"no_event_found":false}

입력: 이번 주 금요일 or 다음 주 화요일 중 편한 시간에
출력: {"events":[{"title":null,"start":{"explicit":null,"relative":{"week_offset":0,"weekday":5},"confidence":0.6},"end":null,"all_day":true,"candidate_group":1},{"title":null,"start":{"explicit":null,"relative":{"week_offset":1,"weekday":2},"confidence":0.6},"end":null,"all_day":true,"candidate_group":1}],"no_event_found":false}

입력: 설 전날 큰집에서 모여요
출력: {"events":[{"title":null,"start":{"explicit":null,"relative":{"holiday":{"name":"설날","day_offset":-1,"whole_period":false}},"confidence":0.85},"end":null,"all_day":true,"location":"큰집","candidate_group":null}],"no_event_found":false}

입력: 어제 보내주신 자료 잘 받았습니다
출력: {"events":[],"no_event_found":true}"#;
