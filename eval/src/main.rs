//! 파싱 회귀 실행기.
//!
//! ```text
//! # 형식 검증
//! cargo run -p eval -- --set eval/ko-500.jsonl
//! # 기록된 모델 출력으로 채점 (CI)
//! cargo run -p eval -- --set <set.jsonl> --outputs <outputs.jsonl> [--min-accuracy 0.95]
//! # 실제 모델을 호출해 출력을 기록한 뒤 채점
//! cargo run -p eval -- --set <set.jsonl> --provider ollama --base-url http://localhost:11434 \
//!     --model gemma4:e2b --record <outputs.jsonl>
//! ```
//!
//! 모델 출력은 기록해서 고정하고, 채점은 항상 기록된 출력으로 한다 (ADR 0003). `--min-accuracy`보다
//! 낮으면 실패로 끝난다 (CLAUDE.md 규칙 7).

use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Duration;

use c2c_core::holidays::HolidayTable;
use c2c_core::providers::{
    CloudProvider, LlmProvider, OllamaProvider, OpenAiCompatProvider, extract_with_retry,
};
use c2c_core::schema::ExtractionResult;
use eval::score::context_for;

const USAGE: &str = "사용법: eval --set <path.jsonl> [--outputs <path.jsonl> | --provider <openai-compat|ollama|cloud> --base-url <url> --model <name> --record <path.jsonl> [--api-key-env <VAR>] [--device <id>] [--timeout-secs <n>]] [--min-accuracy <0~1>]";

#[derive(Default)]
struct Args {
    set: Option<PathBuf>,
    outputs: Option<PathBuf>,
    min_accuracy: f64,
    provider: Option<String>,
    base_url: Option<String>,
    model: Option<String>,
    api_key_env: Option<String>,
    device: Option<String>,
    record: Option<PathBuf>,
    /// PRD 5.6: 로컬 서버 기본 타임아웃 30초
    timeout_secs: Option<u64>,
}

fn parse_args() -> Result<Args, String> {
    let mut a = Args::default();
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        let mut value = || args.next().ok_or(format!("{arg}에 값이 필요하다"));
        match arg.as_str() {
            "--set" => a.set = Some(value()?.into()),
            "--outputs" => a.outputs = Some(value()?.into()),
            "--min-accuracy" => {
                a.min_accuracy = value()?
                    .parse()
                    .map_err(|_| "--min-accuracy에는 0~1 사이 숫자가 필요하다")?
            }
            "--provider" => a.provider = Some(value()?),
            "--base-url" => a.base_url = Some(value()?),
            "--model" => a.model = Some(value()?),
            "--api-key-env" => a.api_key_env = Some(value()?),
            "--device" => a.device = Some(value()?),
            "--record" => a.record = Some(value()?.into()),
            "--timeout-secs" => {
                a.timeout_secs = Some(value()?.parse().map_err(|_| "--timeout-secs는 정수")?)
            }
            other => return Err(format!("알 수 없는 인자: {other}")),
        }
    }
    if a.set.is_none() {
        return Err("--set <path> 가 필요하다".into());
    }
    if a.provider.is_some() && (a.record.is_none() || a.outputs.is_some()) {
        return Err("--provider는 --record와 함께 쓰고, --outputs와는 같이 쓰지 않는다".into());
    }
    Ok(a)
}

fn build_provider(a: &Args) -> Result<Box<dyn LlmProvider>, String> {
    let name = a.provider.as_deref().unwrap_or_default();
    let base_url = a.base_url.as_deref().ok_or("--base-url이 필요하다")?;
    let timeout = Duration::from_secs(a.timeout_secs.unwrap_or(30));
    let model = || a.model.as_deref().ok_or("--model이 필요하다");
    Ok(match name {
        "openai-compat" => {
            let key = match &a.api_key_env {
                Some(var) => {
                    Some(std::env::var(var).map_err(|_| format!("환경 변수 {var}가 없다"))?)
                }
                None => None,
            };
            Box::new(OpenAiCompatProvider::new(base_url, model()?, key, timeout))
        }
        "ollama" => Box::new(OllamaProvider::new(base_url, model()?, timeout)),
        "cloud" => Box::new(CloudProvider::new(
            base_url,
            a.device.as_deref().unwrap_or("eval"),
            timeout,
        )),
        other => return Err(format!("알 수 없는 공급자: {other}")),
    })
}

fn main() -> ExitCode {
    let args = match parse_args() {
        Ok(args) => args,
        Err(e) => {
            eprintln!("{e}\n{USAGE}");
            return ExitCode::from(2);
        }
    };
    let set = args.set.as_ref().expect("parse_args에서 확인");

    let cases = match eval::load_set(set) {
        Ok(cases) => cases,
        Err(errors) => {
            for e in &errors {
                eprintln!("{}:{}: {}", set.display(), e.line, e.message);
            }
            eprintln!("형식 오류 {}건", errors.len());
            return ExitCode::FAILURE;
        }
    };
    println!("{}: {}건, 형식 정상", set.display(), cases.len());

    let outputs: Vec<Result<ExtractionResult, String>> = if args.provider.is_some() {
        let provider = match build_provider(&args) {
            Ok(p) => p,
            Err(e) => {
                eprintln!("{e}\n{USAGE}");
                return ExitCode::from(2);
            }
        };
        let record = args.record.as_ref().expect("parse_args에서 확인");
        let mut lines = Vec::with_capacity(cases.len());
        for (i, case) in cases.iter().enumerate() {
            let result = extract_with_retry(provider.as_ref(), &case.input, &context_for(case));
            if let Err(e) = &result {
                println!("! {}번 케이스 추출 실패: {e}", i + 1);
            }
            lines.push(eval::record::to_line(&result));
        }
        if let Err(e) = std::fs::write(record, lines.join("\n") + "\n") {
            eprintln!("{}: {e}", record.display());
            return ExitCode::FAILURE;
        }
        println!(
            "{}: {}건 기록 ({})",
            record.display(),
            lines.len(),
            provider.name()
        );
        lines.iter().map(|l| eval::record::from_line(l)).collect()
    } else if let Some(path) = &args.outputs {
        match std::fs::read_to_string(path) {
            Ok(content) => content
                .lines()
                .filter(|l| !l.trim().is_empty())
                .map(eval::record::from_line)
                .collect(),
            Err(e) => {
                eprintln!("{}: {e}", path.display());
                return ExitCode::FAILURE;
            }
        }
    } else {
        return ExitCode::SUCCESS;
    };

    if outputs.len() != cases.len() {
        eprintln!(
            "출력 {}줄과 케이스 {}건의 수가 다르다",
            outputs.len(),
            cases.len()
        );
        return ExitCode::FAILURE;
    }

    let holidays = HolidayTable::bundled();
    let mut correct = 0;
    for (i, (case, output)) in cases.iter().zip(&outputs).enumerate() {
        let verdict = output
            .as_ref()
            .map_err(Clone::clone)
            .and_then(|o| eval::score::score_case(case, o, &holidays));
        match verdict {
            Ok(()) => correct += 1,
            Err(reason) => println!("✗ {}번 케이스: {reason}", i + 1),
        }
    }
    let accuracy = if cases.is_empty() {
        1.0
    } else {
        correct as f64 / cases.len() as f64
    };
    println!(
        "정확도 {:.1}% ({correct}/{})",
        accuracy * 100.0,
        cases.len()
    );
    if accuracy < args.min_accuracy {
        eprintln!("기준 {:.1}% 미달", args.min_accuracy * 100.0);
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}
