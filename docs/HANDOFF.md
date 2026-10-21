# Claude Code 이관 가이드

작성일 2026-10-21. 기획 전문은 `PRD.md`, 작업 규칙은 루트 `CLAUDE.md`.

## 1. 프로젝트 위치: 클라우드 vs 로컬

| 기준 | 클라우드 (Claude Code 웹·모바일 세션) | 로컬 (맥북 M2 Pro + Windows PC) |
| --- | --- | --- |
| 환경 격리 | 세션마다 깨끗한 컨테이너, 의존성 오염 없음 | 내 환경 그대로, 설정 누적 |
| 실기기 테스트 | 불가. iOS·macOS 빌드는 Xcode가 있는 Mac이 필요 | 가능. iOS 시뮬레이터·실기기·메뉴바 앱 즉시 확인 |
| 네트워크 | 허용 목록(패키지 레지스트리, GitHub)만. 그 외 사이트·LAN 불가 | 자유. Windows PC의 TabbyAPI(로컬 LLM) 서버에 LAN으로 접근 가능 |
| 병렬 작업 | 세션 여러 개를 동시에 띄워 독립 작업(core 테스트, 평가셋 생성, 문서) 분산 | 터미널 수만큼, 단 머신 자원 공유 |
| 비용 | 컨테이너 시간 + 토큰. 유휴 세션은 회수됨 | 토큰만 |
| 백그라운드 진행 | 자리를 비워도 계속 돌고 알림 | 맥북이 켜져 있어야 함 |
| Rust·Flutter 도구 | 매 세션 설치 필요(캐시 없음). Flutter SDK 설치만 2~3분 | 한 번 설치 |
| 비밀값 | 세션에 주입해야 함. OAuth 시크릿·키체인 접근 불가 | 키체인·`.env` 로컬 보관 |
| 플랫폼 빌드 | Android APK·Windows(cross)·Rust·프록시·백엔드 가능 | 4개 전부 |

**추천: 하이브리드.** 저장소는 GitHub 하나, 작업을 성격으로 나눈다.

- **클라우드 세션:** `core/`(Rust 단위 테스트), `proxy/`, `backend/`, `eval/`(평가셋 생성·회귀), 문서. 실기기가 필요 없고 격리·병렬이 유리한 일. 0~1단계(준비·파싱 코어)의 80%가 여기
- **로컬(맥북):** `app/` Flutter UI, `ios/`·`macos/` 네이티브, 실기기 검증, 스토어 제출. 2단계부터 비중 증가
- **로컬(Windows PC):** `windows/` 트레이·더블 탭 검증, 로컬 LLM 모드 테스트(TabbyAPI → `OpenAiCompatProvider`)
- 규칙: 클라우드 세션은 브랜치에서 PR까지만, 머지는 로컬에서 실기기 확인 후

## 2. 모델 배정

가용 모델: Fable 5.1, Opus 5.5, Sonnet 5.5, Haiku 5.5, Gemini 4 Argon, Gemini 3.1 Pro, Gemini 3.8 Flash.

원칙 세 가지. (1) 작업 단위당 가장 싼 모델이 통과하는지부터 본다. (2) 한 세션 안에서 모델을 바꾸지 말고, 작업 종류로 세션을 나눈다. (3) Gemini는 Claude Code 안에서 쓰는 게 아니라 **교차 검토·대량 생성 보조**로 바깥에서 쓴다 — 서로 다른 계열의 두 모델이 같은 실수를 할 확률이 낮다.

| 작업 | 1순위 | 이유 | 대안 |
| --- | --- | --- | --- |
| 아키텍처 결정, `core/` 타입·트레이트 설계, FFI 경계(`flutter_rust_bridge`·XCFramework) | **Fable 5.1** | 되돌리기 비싼 결정. 세션 수 적음 | Opus 5.5 |
| iOS Share Extension(SwiftUI + Rust), EventKit, macOS Services·더블 탭 | **Fable 5.1** | OS 정책·권한이 얽힌 난제, 문서가 적음 | Opus 5.5 |
| 상대 날짜 해석기, RRULE, 중복 판정 등 `core/` 구현 | **Opus 5.5** | 로직 밀도 높지만 명세(PRD 5.3)가 명확 | Fable 5.1 (막힐 때만) |
| Flutter 화면 구현 (검토 카드, 온보딩, 설정) | **Opus 5.5** | 디자인 시안이 있으면 Sonnet으로 내려도 됨 | Sonnet 5.5 |
| 프록시·Supabase 마이그레이션·RLS·CI 설정 | **Sonnet 5.5** | 정형화된 작업, 레퍼런스 풍부 | Opus 5.5 |
| 테스트 생성, 리팩터링, 린트 수정, l10n 문구 | **Sonnet 5.5** | 반복·대량 | Haiku 5.5 |
| 한국어 일정 메시지 평가셋 초안 500건 생성 | **Haiku 5.5** + **Gemini 3.8 Flash** 절반씩 | 다양성 확보, 비용 최저. 라벨 검수는 JS | — |
| 평가셋 라벨 교차 검증 (두 모델이 다르게 푼 건만 사람이 봄) | **Gemini 3.1 Pro** vs **Opus 5.5** | 계열이 다른 두 모델의 불일치가 곧 어려운 케이스 | Gemini 4 Argon |
| PR 리뷰 (작성 모델과 다른 모델로) | **Gemini 4 Argon** | 작성자가 자기 코드를 리뷰하는 편향 제거 | Fable 5.1 |
| 프롬프트(시스템 프롬프트·few-shot) 반복 개선 | **Opus 5.5** | 평가셋 결과를 보며 반복, 중간 비용 | Fable 5.1 |
| 운영 중 실제 파싱 (제품 안) | Haiku 5.5 | PRD 5.7 | GPT-6 Luna, Gemini 2.5 Flash-Lite |

**비용 감각:** 전체 17주 작업량을 토큰으로 보면 Sonnet·Haiku 작업이 건수로는 60% 이상이지만 비용은 20% 미만이고, Fable 세션은 건수 10% 미만에 비용 절반 이상이다. Fable 세션은 "설계 문서를 쓰고 끝내는" 짧은 세션으로 제한하고, 그 문서를 Opus·Sonnet 세션에 넘긴다.

**Gemini 3.1 Pro를 Claude 대신 쓰지 않는 이유:** `flutter_rust_bridge`·EventKit·Share Extension처럼 레퍼런스가 적은 영역에서 Claude Code의 도구 사용(파일 편집·빌드·테스트 루프)이 더 안정적이다. Gemini는 긴 컨텍스트(평가셋 전체 검토)와 교차 검증에 강점이 있어 그 자리에 둔다.

## 3. 0단계 (11/2 ~ 11/8) 세션 계획

| # | 세션 | 모델 | 위치 | 산출물 | 게이트 |
| --- | --- | --- | --- | --- | --- |
| 0-1 | 모노레포 뼈대, CI 골격, `core/` 크레이트 타입 정의 | Fable 5.1 | 클라우드 | `core/src/schema.rs`, `.github/workflows/ci.yml` | `cargo test` 통과 |
| 0-2 | 평가셋 200건 초안 (Haiku 100 + Gemini Flash 100) | Haiku 5.5 / Gemini 3.8 Flash | 클라우드 / 외부 | `eval/ko-200.jsonl` | JS 라벨 검수 |
| 0-3 | 상대 날짜 해석기 + 표 기반 테스트 | Opus 5.5 | 클라우드 | `core/src/relative.rs`, `core/tests/relative_dates.rs` | PRD 5.3 표 전부 통과 |
| 0-4 | 스택 PoC: Flutter + Rust 브리지로 "텍스트 → 카드 → EventKit" (iOS) | Fable 5.1 | 로컬(맥북) | `app/` 최소 화면, `ios/` 확장 스켈레톤 | 실기기에서 등록 성공 |
| 0-5 | 복사 더블 탭 감지 PoC (macOS·Windows) | Opus 5.5 | 로컬 양쪽 | `macos/DoubleTap.swift`, `windows/double_tap.cpp` | 오탐 없이 400ms 감지 |
| 0-6 | 프록시 골격 + 클라우드·Ollama·OpenAI 호환 어댑터 3종 | Sonnet 5.5 | 클라우드 | `proxy/`, `core/src/providers/` | 샘플 1건 JSON 유효 |
| 0-7 | Claude Design 검토 카드·온보딩 시안 | (Design) | — | 시안 링크를 `docs/design.md`에 | JS 선택 |

0-1이 끝나야 0-3·0-6을 시작한다. 0-2·0-4·0-5·0-7은 독립.

## 4. 첫 세션 킥오프 프롬프트 (0-1, Fable 5.1, 클라우드)

아래를 그대로 붙여 넣는다.

```
docs/PRD.md와 CLAUDE.md를 읽고 시작해.

목표: C2C 모노레포 뼈대와 core 크레이트의 타입 정의를 만들고, CI가 cargo test를 통과하게 한다. UI·플랫폼 코드는 만들지 않는다.

1. 폴더: core/ app/ ios/ android/ macos/ windows/ proxy/ backend/ eval/ — 각 폴더에 README.md 한 줄(역할)만.
2. core/: Cargo 워크스페이스, 크레이트 c2c-core. src/schema.rs에 PRD 5.1의 추출 스키마를 serde 타입으로 — Event, DateTimeField { explicit, relative, confidence }, RelativeExpr, Ambiguity. relative 필드는 PRD 5.3 표의 모든 표현을 담을 수 있게 설계하되, 지금은 타입만.
3. src/providers/mod.rs에 LlmProvider, CalendarProvider 트레이트 (PRD 5.6·7절 시그니처). 구현체는 없음.
4. eval/: ko-500.jsonl 형식 정의(input, expected, reference_date, notes)와 빈 파일, 회귀 스크립트 스켈레톤.
5. .github/workflows/ci.yml: cargo fmt --check, clippy, test.
6. 끝나면 결정이 필요했던 지점(이름, 타입 경계, 애매한 PRD 문구)을 docs/decisions/0001-schema.md에 ADR 형식으로 남겨.

지어내지 말 것: PRD에 없는 필드나 플랫폼 동작은 추가하지 말고 "미결"로 적어.
```

## 5. 세션 넘길 때 규칙

- 세션 간 인수인계는 `docs/decisions/NNNN-*.md`(ADR)와 PR 본문으로만. 채팅 기록을 다음 세션에 붙여 넣지 않는다
- Fable 세션이 남긴 ADR을 Opus·Sonnet 세션 프롬프트 첫 줄에 "읽고 시작해"로 지정
- 실기기 확인 결과는 JS가 PR 코멘트로 남기고, 다음 세션이 그 코멘트를 읽는다
