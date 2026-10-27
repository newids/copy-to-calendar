//! 공급자 공통 HTTP·응답 처리. 오류 메시지에 사용자 메시지 본문을 넣지 않는다 (PRD 9절).

use std::time::Duration;

use serde_json::Value;

use super::LlmError;
use crate::schema::ExtractionResult;

pub(crate) fn agent(timeout: Duration) -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_global(Some(timeout))
        .build()
        .into()
}

fn map_error(e: ureq::Error) -> LlmError {
    match e {
        ureq::Error::StatusCode(429) => LlmError::RateLimited,
        ureq::Error::StatusCode(code) => LlmError::Transport(format!("HTTP {code}")),
        ureq::Error::Timeout(_) => LlmError::Timeout,
        other => LlmError::Transport(other.to_string()),
    }
}

pub(crate) fn post_json(
    agent: &ureq::Agent,
    url: &str,
    headers: &[(&str, &str)],
    body: &Value,
) -> Result<Value, LlmError> {
    let mut request = agent.post(url);
    for (name, value) in headers {
        request = request.header(*name, *value);
    }
    let mut response = request.send_json(body).map_err(map_error)?;
    response
        .body_mut()
        .read_json()
        .map_err(|e| LlmError::InvalidOutput(format!("응답이 JSON이 아님: {e}")))
}

pub(crate) fn get_json(
    agent: &ureq::Agent,
    url: &str,
    headers: &[(&str, &str)],
) -> Result<Value, LlmError> {
    let mut request = agent.get(url);
    for (name, value) in headers {
        request = request.header(*name, *value);
    }
    let mut response = request.call().map_err(map_error)?;
    response
        .body_mut()
        .read_json()
        .map_err(|e| LlmError::InvalidOutput(format!("응답이 JSON이 아님: {e}")))
}

/// 모델이 낸 JSON 문자열을 스키마 타입으로 읽는다.
pub fn parse_output(content: &str) -> Result<ExtractionResult, LlmError> {
    serde_json::from_str(content.trim())
        .map_err(|e| LlmError::InvalidOutput(format!("스키마와 맞지 않음: {e}")))
}

/// 응답 JSON에서 경로를 따라 문자열을 꺼낸다.
pub(crate) fn string_at<'a>(value: &'a Value, pointer: &str) -> Result<&'a str, LlmError> {
    value
        .pointer(pointer)
        .and_then(Value::as_str)
        .ok_or_else(|| LlmError::InvalidOutput(format!("응답에 {pointer} 없음")))
}
