//! 교체 가능한 외부 연동 어댑터 (PRD 5.5·5.6·7절).
//!
//! LLM 어댑터 3종은 모두 같은 프롬프트([`prompt`])와 출력 스키마를 쓴다. 캘린더 어댑터(Google, EventKit)는
//! 2·3단계에서 추가한다. 규칙은 ADR 0004.

mod http;

pub mod cloud;
pub mod ollama;
pub mod openai_compat;
pub mod prompt;

pub use cloud::CloudProvider;
pub use http::parse_output;
pub use ollama::OllamaProvider;
pub use openai_compat::OpenAiCompatProvider;

use chrono::{DateTime, FixedOffset};
use thiserror::Error;

use crate::calendar::{Calendar, CalendarEvent, CalendarId, EventRef, StoredEvent};
use crate::context::ParseContext;
use crate::schema::ExtractionResult;

/// 메시지 텍스트를 PRD 5.1 스키마로 추출하는 LLM 공급자.
///
/// 날짜를 확정하지 않는다. 반환값의 `relative`는 후처리 코드가 해석한다.
/// 파싱 실패 시 1회 재시도·규칙 기반 폴백은 호출하는 쪽이 맡는다 (PRD 5.1·5.6).
pub trait LlmProvider: Send + Sync {
    /// 평가 리포트·설정 화면에 쓰는 식별자.
    fn name(&self) -> &str;

    fn extract(&self, text: &str, context: &ParseContext) -> Result<ExtractionResult, LlmError>;
}

/// 출력이 스키마와 맞지 않으면 1회 다시 요청한다 (PRD 5.1·5.6). 그래도 실패하면 오류를 돌려주고,
/// 호출하는 쪽이 수동 입력 또는 규칙 기반 폴백으로 넘어간다.
pub fn extract_with_retry(
    provider: &dyn LlmProvider,
    text: &str,
    context: &ParseContext,
) -> Result<ExtractionResult, LlmError> {
    match provider.extract(text, context) {
        Err(LlmError::InvalidOutput(_)) => provider.extract(text, context),
        other => other,
    }
}

/// 오류 메시지에 사용자 메시지 본문을 넣지 않는다 (PRD 9절).
#[derive(Debug, Error)]
pub enum LlmError {
    #[error("전송 실패: {0}")]
    Transport(String),
    #[error("시간 초과")]
    Timeout,
    #[error("요청 한도 초과")]
    RateLimited,
    /// JSON 파싱 또는 스키마 검증 실패.
    #[error("응답이 스키마와 맞지 않음: {0}")]
    InvalidOutput(String),
}

/// 캘린더 읽기·쓰기 (PRD 7절). 구현 2종: Google Calendar API v3, EventKit.
///
/// 쓰기(`create_event`·`update_event`·`delete_event`)는 사용자가 검토 카드에서 확인한 뒤에만
/// 호출한다. 확인 없이 쓰는 경로를 만들지 않는다 (PRD 2절 제외 범위).
pub trait CalendarProvider: Send + Sync {
    fn list_calendars(&self) -> Result<Vec<Calendar>, CalendarError>;

    /// `[start, end)`와 겹치는 기존 일정 (PRD 4.3 충돌 경고).
    fn find_conflicts(
        &self,
        start: DateTime<FixedOffset>,
        end: DateTime<FixedOffset>,
    ) -> Result<Vec<StoredEvent>, CalendarError>;

    /// 같은 제목 + 시작 시각 ±15분인 기존 일정 (PRD 4.4 중복 감지).
    fn find_duplicate(
        &self,
        title: &str,
        start: DateTime<FixedOffset>,
    ) -> Result<Option<StoredEvent>, CalendarError>;

    fn create_event(
        &self,
        calendar: &CalendarId,
        event: &CalendarEvent,
    ) -> Result<EventRef, CalendarError>;

    fn update_event(&self, id: &EventRef, event: &CalendarEvent) -> Result<(), CalendarError>;

    /// 등록 직후 "실행 취소"에도 쓴다 (PRD 4.4).
    fn delete_event(&self, id: &EventRef) -> Result<(), CalendarError>;
}

#[derive(Debug, Error)]
pub enum CalendarError {
    /// 캘린더 권한 없음 또는 OAuth 토큰 만료·폐기.
    #[error("권한 없음: {0}")]
    Unauthorized(String),
    #[error("이벤트 또는 캘린더를 찾을 수 없음")]
    NotFound,
    #[error("전송 실패: {0}")]
    Transport(String),
    #[error("캘린더 오류: {0}")]
    Other(String),
}
