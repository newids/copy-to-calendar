//! OpenAI 호환 서버 (LM Studio, vLLM, llama.cpp server 등) 어댑터 (PRD 5.6).
//! `response_format: json_schema`로 출력 스키마를 강제한다.

use std::time::Duration;

use serde_json::{Value, json};

use super::http::{self, parse_output, string_at};
use super::prompt::{Prompt, extraction_schema};
use super::{LlmError, LlmProvider};
use crate::context::ParseContext;
use crate::schema::ExtractionResult;

pub struct OpenAiCompatProvider {
    /// 예: `http://localhost:1234/v1` (LM Studio). `/chat/completions`를 붙여 호출한다.
    base_url: String,
    model: String,
    api_key: Option<String>,
    agent: ureq::Agent,
}

impl OpenAiCompatProvider {
    pub fn new(base_url: &str, model: &str, api_key: Option<String>, timeout: Duration) -> Self {
        Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            model: model.to_string(),
            api_key,
            agent: http::agent(timeout),
        }
    }

    fn headers(&self) -> Vec<(&str, String)> {
        self.api_key
            .iter()
            .map(|key| ("Authorization", format!("Bearer {key}")))
            .collect()
    }

    /// 설정 화면 드롭다운용 모델 목록 (`GET /models`).
    pub fn list_models(&self) -> Result<Vec<String>, LlmError> {
        let headers = self.headers();
        let headers: Vec<(&str, &str)> = headers.iter().map(|(k, v)| (*k, v.as_str())).collect();
        let body = http::get_json(&self.agent, &format!("{}/models", self.base_url), &headers)?;
        Ok(body["data"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|m| m["id"].as_str().map(str::to_string))
            .collect())
    }

    pub fn request_body(&self, prompt: &Prompt) -> Value {
        json!({
            "model": self.model,
            "temperature": 0,
            "messages": [
                {"role": "system", "content": prompt.system()},
                {"role": "user", "content": prompt.text},
            ],
            "response_format": {
                "type": "json_schema",
                "json_schema": {"name": "extraction", "strict": true, "schema": extraction_schema()},
            },
        })
    }
}

impl LlmProvider for OpenAiCompatProvider {
    fn name(&self) -> &str {
        "openai-compat"
    }

    fn extract(&self, text: &str, context: &ParseContext) -> Result<ExtractionResult, LlmError> {
        let prompt = Prompt::new(text, context);
        let headers = self.headers();
        let headers: Vec<(&str, &str)> = headers.iter().map(|(k, v)| (*k, v.as_str())).collect();
        let response = http::post_json(
            &self.agent,
            &format!("{}/chat/completions", self.base_url),
            &headers,
            &self.request_body(&prompt),
        )?;
        parse_output(string_at(&response, "/choices/0/message/content")?)
    }
}
