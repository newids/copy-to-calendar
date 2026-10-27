//! 파싱 회귀 실행기.
//!
//! ```text
//! cargo run -p eval -- --set eval/ko-500.jsonl                                  # 형식 검증만
//! cargo run -p eval -- --set <set.jsonl> --outputs <outputs.jsonl> [--min-accuracy 0.95]
//! cargo run -p eval -- --set eval/ko-500.jsonl --provider <name>                # 0-6 이후: 출력 기록
//! ```
//!
//! `--outputs`는 케이스와 같은 줄 순서로 모델 출력(PRD 5.1 JSON)을 담은 JSONL이다. 출력을 core로 조립해
//! `expected`와 비교한다. `--min-accuracy`보다 낮으면 실패로 끝난다 (CLAUDE.md 규칙 7).
//! 모델 출력은 기록해서 고정하고, 채점은 항상 기록된 출력으로 한다 (ADR 0003).

use std::path::PathBuf;
use std::process::ExitCode;

use c2c_core::holidays::HolidayTable;
use c2c_core::schema::ExtractionResult;

struct Args {
    set: PathBuf,
    outputs: Option<PathBuf>,
    min_accuracy: f64,
    provider: Option<String>,
}

fn parse_args() -> Result<Args, String> {
    let mut set = None;
    let mut outputs = None;
    let mut min_accuracy = 0.0;
    let mut provider = None;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--set" => set = args.next().map(PathBuf::from),
            "--outputs" => outputs = args.next().map(PathBuf::from),
            "--min-accuracy" => {
                min_accuracy = args
                    .next()
                    .and_then(|v| v.parse().ok())
                    .ok_or("--min-accuracy에는 0~1 사이 숫자가 필요하다")?
            }
            "--provider" => provider = args.next(),
            other => return Err(format!("알 수 없는 인자: {other}")),
        }
    }
    let set = set.ok_or("--set <path> 가 필요하다")?;
    Ok(Args {
        set,
        outputs,
        min_accuracy,
        provider,
    })
}

fn main() -> ExitCode {
    let args = match parse_args() {
        Ok(args) => args,
        Err(e) => {
            eprintln!(
                "{e}\n사용법: eval --set <path.jsonl> [--outputs <path.jsonl> [--min-accuracy <0~1>]] [--provider <name>]"
            );
            return ExitCode::from(2);
        }
    };

    let cases = match eval::load_set(&args.set) {
        Ok(cases) => cases,
        Err(errors) => {
            for e in &errors {
                eprintln!("{}:{}: {}", args.set.display(), e.line, e.message);
            }
            eprintln!("형식 오류 {}건", errors.len());
            return ExitCode::FAILURE;
        }
    };
    println!("{}: {}건, 형식 정상", args.set.display(), cases.len());

    if let Some(provider) = args.provider {
        eprintln!("공급자 '{provider}'는 아직 구현되지 않았다 (0-6 세션)");
        return ExitCode::FAILURE;
    }
    let Some(outputs_path) = args.outputs else {
        return ExitCode::SUCCESS;
    };

    let outputs = match std::fs::read_to_string(&outputs_path) {
        Ok(content) => content
            .lines()
            .filter(|l| !l.trim().is_empty())
            .map(|l| serde_json::from_str::<ExtractionResult>(l).map_err(|e| e.to_string()))
            .collect::<Vec<_>>(),
        Err(e) => {
            eprintln!("{}: {e}", outputs_path.display());
            return ExitCode::FAILURE;
        }
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
            .map_err(|e| format!("출력이 스키마와 맞지 않음: {e}"))
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
