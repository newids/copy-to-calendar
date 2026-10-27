//! LLM 어댑터 테스트 (ADR 0004). 실제 모델 대신 로컬 목 서버로 요청·응답 형식을 검증한다.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use c2c_core::context::ParseContext;
use c2c_core::providers::prompt::{Prompt, extraction_schema};
use c2c_core::providers::{
    CloudProvider, LlmError, LlmProvider, OllamaProvider, OpenAiCompatProvider, extract_with_retry,
    parse_output,
};
use c2c_core::schema::ExtractionResult;
use serde_json::{Value, json};

const TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Debug)]
struct Captured {
    method: String,
    path: String,
    headers: Vec<(String, String)>,
    body: Value,
}

impl Captured {
    fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.as_str())
    }
}

/// 요청 1건을 받아 기록하고 준비된 응답을 돌려주는 목 서버. (base URL, 기록 수신기)
fn serve(status: u16, response: String, delay: Duration) -> (String, mpsc::Receiver<Captured>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut reader = BufReader::new(stream.try_clone().unwrap());
        let mut request_line = String::new();
        reader.read_line(&mut request_line).unwrap();
        let mut parts = request_line.split_whitespace();
        let method = parts.next().unwrap_or_default().to_string();
        let path = parts.next().unwrap_or_default().to_string();
        let mut headers = Vec::new();
        let mut length = 0;
        loop {
            let mut line = String::new();
            reader.read_line(&mut line).unwrap();
            let line = line.trim_end();
            if line.is_empty() {
                break;
            }
            let (k, v) = line.split_once(':').unwrap();
            let (k, v) = (k.trim().to_ascii_lowercase(), v.trim().to_string());
            if k == "content-length" {
                length = v.parse().unwrap();
            }
            headers.push((k, v));
        }
        let mut raw = vec![0; length];
        reader.read_exact(&mut raw).unwrap();
        let body = serde_json::from_slice(&raw).unwrap_or(Value::Null);
        let _ = tx.send(Captured {
            method,
            path,
            headers,
            body,
        });
        thread::sleep(delay);
        let _ = write!(
            stream,
            "HTTP/1.1 {status} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{response}",
            response.len()
        );
    });
    (base, rx)
}

fn ok(response: Value) -> (String, mpsc::Receiver<Captured>) {
    serve(200, response.to_string(), Duration::ZERO)
}

fn context() -> ParseContext {
    serde_json::from_value(json!({"reference": "2026-10-19T09:00:00", "time_zone": "Asia/Seoul"}))
        .unwrap()
}

const OUTPUT: &str = r#"{"events":[{"title":null,"start":{"explicit":null,"relative":{"day_offset":1,"week_offset":null,"weekday":null,"month_offset":null,"month":null,"day":null,"month_period":null,"holiday":null,"time_word":"점심","hour":null,"minute":null},"confidence":0.9},"end":null,"all_day":false,"location":null,"attendees":[],"recurrence":null,"notes":"내일 점심","ambiguities":[],"candidate_group":null}],"no_event_found":false}"#;

fn expected_output() -> ExtractionResult {
    serde_json::from_str(OUTPUT).unwrap()
}

#[test]
fn openai_compat_request_and_response() {
    let (base, rx) = ok(json!({"choices": [{"message": {"content": OUTPUT}}]}));
    let provider =
        OpenAiCompatProvider::new(&format!("{base}/v1/"), "qwen", Some("k".into()), TIMEOUT);
    let result = provider.extract("내일 점심", &context()).unwrap();
    assert_eq!(result, expected_output());

    let req = rx.recv().unwrap();
    assert_eq!(
        (req.method.as_str(), req.path.as_str()),
        ("POST", "/v1/chat/completions")
    );
    assert_eq!(req.header("authorization"), Some("Bearer k"));
    assert_eq!(req.body["model"], "qwen");
    assert_eq!(req.body["temperature"], 0);
    assert_eq!(req.body["messages"][1]["content"], "내일 점심");
    let system = req.body["messages"][0]["content"].as_str().unwrap();
    assert!(system.contains("날짜를 계산하지 마라") && system.contains("2026-10-19 (월) 09:00"));
    let format = &req.body["response_format"];
    assert_eq!(format["type"], "json_schema");
    assert_eq!(format["json_schema"]["strict"], true);
    assert_eq!(format["json_schema"]["schema"], extraction_schema());
}

#[test]
fn openai_compat_without_key_sends_no_authorization() {
    let (base, rx) = ok(json!({"choices": [{"message": {"content": OUTPUT}}]}));
    OpenAiCompatProvider::new(&base, "m", None, TIMEOUT)
        .extract("x", &context())
        .unwrap();
    assert_eq!(rx.recv().unwrap().header("authorization"), None);
}

#[test]
fn ollama_request_and_response() {
    let (base, rx) = ok(json!({"message": {"role": "assistant", "content": OUTPUT}}));
    let result = OllamaProvider::new(&base, "gemma4:e2b", TIMEOUT)
        .extract("내일 점심", &context())
        .unwrap();
    assert_eq!(result, expected_output());

    let req = rx.recv().unwrap();
    assert_eq!(req.path, "/api/chat");
    assert_eq!(req.body["model"], "gemma4:e2b");
    assert_eq!(req.body["stream"], false);
    assert_eq!(req.body["format"], extraction_schema());
    assert_eq!(req.body["messages"][1]["content"], "내일 점심");
}

#[test]
fn cloud_request_and_response() {
    let (base, rx) = ok(serde_json::from_str(OUTPUT).unwrap());
    let result = CloudProvider::new(&base, "device-1", TIMEOUT)
        .extract("내일 점심", &context())
        .unwrap();
    assert_eq!(result, expected_output());

    let req = rx.recv().unwrap();
    assert_eq!(req.path, "/v1/extract");
    assert_eq!(req.header("x-c2c-device"), Some("device-1"));
    let prompt = Prompt::new("내일 점심", &context());
    assert_eq!(req.body["instructions"], prompt.instructions.as_str());
    assert_eq!(req.body["context"], prompt.context.as_str());
    assert_eq!(req.body["text"], "내일 점심");
}

#[test]
fn list_models() {
    let (base, rx) = ok(json!({"data": [{"id": "a"}, {"id": "b"}]}));
    let models = OpenAiCompatProvider::new(&base, "m", None, TIMEOUT)
        .list_models()
        .unwrap();
    assert_eq!(models, ["a", "b"]);
    assert_eq!(rx.recv().unwrap().path, "/models");

    let (base, rx) = ok(json!({"models": [{"name": "gemma4:e2b"}]}));
    let models = OllamaProvider::new(&base, "m", TIMEOUT)
        .list_models()
        .unwrap();
    assert_eq!(models, ["gemma4:e2b"]);
    assert_eq!(rx.recv().unwrap().path, "/api/tags");
}

#[test]
fn errors() {
    let secret = "비밀 메시지 본문";
    let (base, _rx) = serve(429, "{}".into(), Duration::ZERO);
    let err = CloudProvider::new(&base, "d", TIMEOUT)
        .extract(secret, &context())
        .unwrap_err();
    assert!(matches!(err, LlmError::RateLimited));

    let (base, _rx) = serve(500, "{}".into(), Duration::ZERO);
    let err = CloudProvider::new(&base, "d", TIMEOUT)
        .extract(secret, &context())
        .unwrap_err();
    assert!(matches!(err, LlmError::Transport(_)), "{err:?}");
    assert!(
        !err.to_string().contains(secret),
        "오류에 본문이 들어가면 안 된다"
    );

    let (base, _rx) = ok(json!({"choices": [{"message": {"content": "일정이 없습니다"}}]}));
    let err = OpenAiCompatProvider::new(&base, "m", None, TIMEOUT)
        .extract(secret, &context())
        .unwrap_err();
    assert!(matches!(err, LlmError::InvalidOutput(_)), "{err:?}");

    let (base, _rx) = ok(json!({"unexpected": true}));
    let err = OllamaProvider::new(&base, "m", TIMEOUT)
        .extract(secret, &context())
        .unwrap_err();
    assert!(matches!(err, LlmError::InvalidOutput(_)), "{err:?}");

    let (base, _rx) = serve(200, "{}".into(), Duration::from_secs(2));
    let err = OllamaProvider::new(&base, "m", Duration::from_millis(200))
        .extract(secret, &context())
        .unwrap_err();
    assert!(matches!(err, LlmError::Timeout), "{err:?}");
}

struct Flaky {
    calls: AtomicUsize,
    failures: usize,
}

impl LlmProvider for Flaky {
    fn name(&self) -> &str {
        "flaky"
    }

    fn extract(&self, _: &str, _: &ParseContext) -> Result<ExtractionResult, LlmError> {
        if self.calls.fetch_add(1, Ordering::SeqCst) < self.failures {
            Err(LlmError::InvalidOutput("x".into()))
        } else {
            Ok(expected_output())
        }
    }
}

#[test]
fn retries_once_on_invalid_output() {
    let once = Flaky {
        calls: AtomicUsize::new(0),
        failures: 1,
    };
    assert!(extract_with_retry(&once, "x", &context()).is_ok());
    assert_eq!(once.calls.load(Ordering::SeqCst), 2);

    let twice = Flaky {
        calls: AtomicUsize::new(0),
        failures: 2,
    };
    assert!(extract_with_retry(&twice, "x", &context()).is_err());
    assert_eq!(twice.calls.load(Ordering::SeqCst), 2);
}

/// 스키마의 속성 이름과 serde 타입의 필드 이름이 같은지 (양방향).
#[test]
fn schema_matches_types() {
    let schema = extraction_schema();
    let full: ExtractionResult = serde_json::from_value(json!({
        "events": [{
            "title": "t",
            "start": {
                "explicit": {"date": "2026-10-24", "time": "10:00"},
                "relative": {"day_offset": 1, "week_offset": 1, "weekday": 1, "month_offset": 1,
                             "month": 1, "day": 1, "month_period": "월초",
                             "holiday": {"name": "설날", "day_offset": 0, "whole_period": false},
                             "time_word": "오전", "hour": 1, "minute": 1},
                "confidence": 0.5
            },
            "end": null, "all_day": false, "location": "l", "attendees": ["a"],
            "recurrence": "r", "notes": "n",
            "ambiguities": [{"field": "f", "reason": "am_pm_unknown", "alternatives": ["x"]}],
            "candidate_group": 1
        }],
        "no_event_found": false
    }))
    .unwrap();
    let value = serde_json::to_value(&full).unwrap();

    fn resolve<'a>(schema: &'a Value, root: &'a Value) -> &'a Value {
        if let Some(r) = schema["$ref"].as_str() {
            return resolve(root.pointer(&r[1..]).unwrap(), root);
        }
        schema
    }
    fn object_schema<'a>(schema: &'a Value, root: &'a Value) -> Option<&'a Value> {
        let s = resolve(schema, root);
        if s["type"] == "object" {
            return Some(s);
        }
        s["anyOf"]
            .as_array()?
            .iter()
            .find_map(|alt| object_schema(alt, root))
    }
    fn check(value: &Value, schema: &Value, root: &Value, path: &str) {
        match value {
            Value::Object(map) => {
                let s = object_schema(schema, root)
                    .unwrap_or_else(|| panic!("{path}: 객체 스키마 없음"));
                let mut props: Vec<&String> = s["properties"].as_object().unwrap().keys().collect();
                let mut required: Vec<&str> = s["required"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|v| v.as_str().unwrap())
                    .collect();
                let mut keys: Vec<&String> = map.keys().collect();
                props.sort();
                keys.sort();
                required.sort();
                assert_eq!(keys, props, "{path}: 타입 필드와 스키마 속성이 다름");
                assert_eq!(
                    required,
                    props.iter().map(|p| p.as_str()).collect::<Vec<_>>(),
                    "{path}: strict 모드는 모든 속성이 required"
                );
                for (k, v) in map {
                    check(v, &s["properties"][k], root, &format!("{path}.{k}"));
                }
            }
            Value::Array(items) => {
                let s = resolve(schema, root);
                for item in items {
                    check(item, &s["items"], root, &format!("{path}[]"));
                }
            }
            _ => {}
        }
    }
    check(&value, &schema, &schema, "$");
}

#[test]
fn prompt_examples_are_valid_outputs() {
    let instructions = Prompt::new("x", &context()).instructions;
    let outputs: Vec<&str> = instructions
        .lines()
        .filter_map(|l| l.strip_prefix("출력: "))
        .collect();
    assert_eq!(outputs.len(), 6);
    for output in outputs {
        parse_output(output).unwrap_or_else(|e| panic!("{output}: {e}"));
    }
    assert!(instructions.contains("설날") && instructions.contains("추석"));
    assert!(!instructions.contains("대체공휴일"));
}

#[test]
fn prompt_context() {
    let mut ctx = context();
    ctx.recent_locations = (1..=12).map(|i| format!("장소{i}")).collect();
    let prompt = Prompt::new("x", &ctx);
    assert!(
        prompt
            .context
            .contains("기준 일시(오늘): 2026-10-19 (월) 09:00")
    );
    assert!(prompt.context.contains("시간대: Asia/Seoul"));
    assert!(prompt.context.contains("근무시간: 09:00~18:00"));
    assert!(prompt.context.contains("장소10") && !prompt.context.contains("장소11"));
    assert_eq!(
        prompt.system(),
        format!("{}\n\n{}", prompt.instructions, prompt.context)
    );
}
