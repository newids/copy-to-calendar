//! 파싱 회귀 실행기.
//!
//! ```text
//! cargo run -p eval -- --set eval/ko-500.jsonl                 # 형식 검증만
//! cargo run -p eval -- --set eval/ko-500.jsonl --provider <name>
//! ```
//!
//! 공급자 어댑터(0-6)와 상대 날짜 해석기(0-3)가 들어오면 케이스별 채점을 붙인다.

use std::path::PathBuf;
use std::process::ExitCode;

struct Args {
    set: PathBuf,
    provider: Option<String>,
}

fn parse_args() -> Result<Args, String> {
    let mut set = None;
    let mut provider = None;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--set" => set = args.next().map(PathBuf::from),
            "--provider" => provider = args.next(),
            other => return Err(format!("알 수 없는 인자: {other}")),
        }
    }
    let set = set.ok_or("--set <path> 가 필요하다")?;
    Ok(Args { set, provider })
}

fn main() -> ExitCode {
    let args = match parse_args() {
        Ok(args) => args,
        Err(e) => {
            eprintln!("{e}\n사용법: eval --set <path.jsonl> [--provider <name>]");
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
    ExitCode::SUCCESS
}
