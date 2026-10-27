# C2C (AI Copy To Calendar) — Claude Code 작업 규칙

메시지 일정 문구 → LLM 추출 → Google/Apple Calendar 등록. macOS·Windows·iOS·Android 동시 출시.
기획 전문은 `docs/PRD.md`. 이 파일과 충돌하면 PRD가 우선하고, 충돌 사실을 보고한다.

## 스택 (확정, 0단계 PoC로 번복 가능)

- `core/` Rust 크레이트 `c2c-core`: 추출 스키마 타입, 상대 날짜 확정, 시간대·공휴일, RRULE, 중복·충돌 판정, `LlmProvider`·`CalendarProvider` 어댑터, SQLite 히스토리
- `app/` Flutter: UI·검토 카드·온보딩·설정·계정. Riverpod + freezed. Rust는 `flutter_rust_bridge`
- `ios/` Share Extension은 SwiftUI 카드 1장 + `c2c-core` XCFramework. 본 앱을 깨우지 않는다. Flutter 엔진 미탑재
- `android/` Share Activity(Kotlin). `macos/` 메뉴바·Services·복사 더블 탭(Swift). `windows/` 트레이·복사 더블 탭
- `proxy/` 서버리스 파싱 프록시 1개 (Cloudflare Workers). 본문 미저장
- `backend/` Supabase(Auth + Postgres RLS) 마이그레이션, RevenueCat 연동
- `eval/` 한국어 일정 메시지 평가셋 + 회귀 스크립트

## 절대 규칙

1. 사용자 확인 없이 캘린더에 쓰지 않는다. 자동 등록 코드 경로를 만들지 않는다
2. 모델이 계산한 날짜를 믿지 않는다. LLM은 `explicit` 또는 `relative`를 내고, 날짜 확정은 `core/`의 결정적 코드가 한다 (PRD 5.1·5.3)
3. 주 시작은 기본 월요일(ISO-8601), 설정으로 변경 가능. "담주"는 다음 주 시작 요일부터. 연도 없는 날짜의 연도도 코드가 정한다
4. 프록시·백엔드 로그에 메시지 본문을 남기지 않는다. 히스토리 서버 저장은 옵트인 + AES-GCM
5. 데스크톱에서 Accessibility 권한·클립보드 폴링을 쓰지 않는다. 진입점은 복사 더블 탭, Services, 트레이, 사용자 지정 단축키
6. 전역 단축키 기본값 없음. `Ctrl+Shift+C`·`⌘⇧C`는 금지
7. 평가셋 정확도가 떨어지는 변경은 머지하지 않는다 (`eval/` 회귀 CI 필수 통과)
8. 수치·날짜·가격을 지어내지 않는다. 모르면 `[__]`로 두고 보고

## 작업 방식

- 작업 전에 `docs/PRD.md`의 해당 절을 읽고, 절 번호를 커밋 메시지에 적는다 (`feat(core): relative date resolver (PRD 5.3)`)
- 플랫폼 코드는 JS가 실기기에서 검증한다. "실기기 확인 필요" 항목을 PR 본문에 체크리스트로 남긴다
- 한국어 UI 문구는 `app/lib/l10n/ko.arb`에만 둔다. 번역투·조사 오류를 리뷰 항목에 포함
- 비밀값(API 키, OAuth 클라이언트 시크릿)은 코드·커밋에 넣지 않는다. `.env.example`만 커밋
- 테스트: `core/`는 단위 테스트 필수, 상대 날짜 해석은 표 기반 테스트(`core/tests/relative_dates.rs`)
- Rust 빌드가 깨지면 Flutter·iOS 작업을 시작하지 않는다 (의존 순서: core → app → 플랫폼)

## 모델 라우팅

`docs/HANDOFF.md`의 표를 따른다. 요약: 설계·난제는 Fable 5.1, 일상 구현은 Opus 5.5, 반복 작업·테스트 생성은 Sonnet 5.5, 평가셋 생성·문구 검토는 Haiku 5.5. 비용이 큰 모델로 단순 작업을 하지 않는다.

## 명령

```
cargo test -p c2c-core          # core 단위 테스트
cargo run -p eval -- --set <set> --outputs <기록된 출력> --min-accuracy 0.95   # 파싱 회귀
cargo run -p eval -- --set <set> --provider <openai-compat|ollama|cloud> --base-url <url> --model <m> --record <출력>   # 모델 출력 기록
cd proxy && npm ci && npm test && npm run typecheck   # 파싱 프록시
cd app && flutter test               # Flutter 테스트
cd app && flutter build macos|windows|ios|apk
```
