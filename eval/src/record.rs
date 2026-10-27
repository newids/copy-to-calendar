//! 모델 출력 기록 파일 (ADR 0003 3절, ADR 0004). 한 줄이 케이스 하나의 출력이다:
//! 성공하면 PRD 5.1 JSON, 실패하면 `{"error": "..."}`.

use c2c_core::providers::LlmError;
use c2c_core::schema::ExtractionResult;
use serde_json::{Value, json};

pub fn to_line(result: &Result<ExtractionResult, LlmError>) -> String {
    match result {
        Ok(output) => serde_json::to_string(output).expect("직렬화"),
        Err(e) => json!({"error": e.to_string()}).to_string(),
    }
}

/// 기록 한 줄을 읽는다. 실패 기록이나 스키마와 맞지 않는 줄은 이유를 돌려준다 (오답 처리).
pub fn from_line(line: &str) -> Result<ExtractionResult, String> {
    let value: Value = serde_json::from_str(line).map_err(|e| format!("JSON 아님: {e}"))?;
    if let Some(error) = value.get("error") {
        return Err(format!("추출 실패: {}", error.as_str().unwrap_or_default()));
    }
    serde_json::from_value(value).map_err(|e| format!("출력이 스키마와 맞지 않음: {e}"))
}
