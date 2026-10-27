//! Ollama 어댑터 (PRD 5.6). Ollama의 OpenAI 호환 엔드포인트는 `json_schema`를 무시하므로
//! 네이티브 `/api/chat`의 `format` 필드에 스키마를 넣는다.

use std::time::Duration;

use serde_json::{Value, json};

use super::http::{self, parse_output, string_at};
use super::prompt::{Prompt, extraction_schema};
use super::{LlmError, LlmProvider};
use crate::context::ParseContext;
use crate::schema::ExtractionResult;

pub struct OllamaProvider {
    /// 예: `http://localhost:11434`
    base_url: String,
    model: String,
    agent: ureq::Agent,
}

impl OllamaProvider {
    pub fn new(base_url: &str, model: &str, timeout: Duration) -> Self {
        Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            model: model.to_string(),
            agent: http::agent(timeout),
        }
    }

    /// 설정 화면 드롭다운용 모델 목록 (`GET /api/tags`).
    pub fn list_models(&self) -> Result<Vec<String>, LlmError> {
        let body = http::get_json(&self.agent, &format!("{}/api/tags", self.base_url), &[])?;
        Ok(body["models"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|m| m["name"].as_str().map(str::to_string))
            .collect())
    }

    pub fn request_body(&self, prompt: &Prompt) -> Value {
        json!({
            "model": self.model,
            "stream": false,
            "format": extraction_schema(),
            "options": {"temperature": 0},
            "messages": [
                {"role": "system", "content": prompt.system()},
                {"role": "user", "content": prompt.text},
            ],
        })
    }
}

impl LlmProvider for OllamaProvider {
    fn name(&self) -> &str {
        "ollama"
    }

    fn extract(&self, text: &str, context: &ParseContext) -> Result<ExtractionResult, LlmError> {
        let prompt = Prompt::new(text, context);
        let response = http::post_json(
            &self.agent,
            &format!("{}/api/chat", self.base_url),
            &[],
            &self.request_body(&prompt),
        )?;
        parse_output(string_at(&response, "/message/content")?)
    }
}
