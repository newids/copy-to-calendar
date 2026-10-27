//! 파싱 프록시 어댑터 (PRD 8.2). LLM API 키는 프록시에만 있고, 앱은 프롬프트만 보낸다.
//! 프록시 계약은 `proxy/README.md`.

use std::time::Duration;

use serde_json::{Value, json};

use super::http;
use super::prompt::Prompt;
use super::{LlmError, LlmProvider};
use crate::context::ParseContext;
use crate::schema::ExtractionResult;

pub struct CloudProvider {
    /// 예: `https://proxy.example.workers.dev`. `/v1/extract`를 붙여 호출한다.
    base_url: String,
    /// 기기별 익명 ID. 프록시의 레이트 리밋 키다 (PRD 9절).
    device_id: String,
    agent: ureq::Agent,
}

impl CloudProvider {
    pub fn new(base_url: &str, device_id: &str, timeout: Duration) -> Self {
        Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            device_id: device_id.to_string(),
            agent: http::agent(timeout),
        }
    }

    pub fn request_body(prompt: &Prompt) -> Value {
        json!({
            "instructions": prompt.instructions,
            "context": prompt.context,
            "text": prompt.text,
        })
    }
}

impl LlmProvider for CloudProvider {
    fn name(&self) -> &str {
        "cloud"
    }

    fn extract(&self, text: &str, context: &ParseContext) -> Result<ExtractionResult, LlmError> {
        let prompt = Prompt::new(text, context);
        let response = http::post_json(
            &self.agent,
            &format!("{}/v1/extract", self.base_url),
            &[("X-C2C-Device", self.device_id.as_str())],
            &Self::request_body(&prompt),
        )?;
        serde_json::from_value(response)
            .map_err(|e| LlmError::InvalidOutput(format!("스키마와 맞지 않음: {e}")))
    }
}
