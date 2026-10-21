# 0001. 명칭·모노레포·추출 스키마·어댑터 경계

- 상태: 채택 (0-1 세션, 2026-10-21)
- 관련: PRD 4.2·5.1·5.2·5.3·5.6·7·8.4, HANDOFF 3·4절

## 배경

0-1 세션의 목표는 모노레포 뼈대와 `core` 크레이트의 타입 정의, 그리고 `cargo test`를 통과하는 CI다. 타입을 정하다 보니 PRD가 정하지 않았거나 문구끼리 어긋나는 지점이 나왔다. 아래 "결정"은 이번 세션에서 정했고 되돌릴 수 있는 것, "미결"은 정하지 않고 남긴 것이다.

## 결정

### 1. 서비스명과 코드 식별자

- 서비스명은 **AI Copy To Calendar**, 약칭·코드명은 **C2C**다 (2026-10-21, JS 지시). 가칭 PasteCal을 대체한다
- Rust 크레이트는 `pastecal-core` 대신 `c2c-core`(라이브러리 경로 `c2c_core`)로 한다. 저장소 이름은 `copy-to-calendar`
- PRD 1절 "가칭"과 13절 "앱 최종 명칭" 미결 항목을 이에 맞춰 고쳤다. 아이콘 방향은 계속 미결이다

### 2. Cargo 워크스페이스는 저장소 루트에 둔다

HANDOFF 4절은 "core/: Cargo 워크스페이스"라고 쓰지만, CLAUDE.md의 `cargo run -p eval ...`이 동작하려면 `eval`도 같은 워크스페이스 멤버여야 한다. 그래서 루트 `Cargo.toml`을 워크스페이스로 두고 멤버를 `core`·`eval`로 했다. 의존성 버전은 `[workspace.dependencies]`에서 한 번만 정한다. `Cargo.lock`은 커밋한다 (`eval`이 바이너리라서).

### 3. 추출 스키마 (`core/src/schema.rs`)

PRD 5.1의 JSON을 그대로 받도록 serde 타입을 만들었다. PRD 예시 두 개는 `core/tests/schema.rs`에서 그대로 파싱되고, 직렬화해서 다시 읽어도 같은 값이 나온다.

| 결정 | 이유 |
| --- | --- |
| `title`은 `Option<String>` | PRD 5.2 "추출하지 않은 정보는 null". 제목이 없다고 응답 전체를 버리지 않고 카드에서 입력받는다 |
| `end`는 `Option<DateTimeField>` | 없으면 후처리에서 +1시간 (PRD 4.2) |
| `explicit = {date, time}`에서 `date`는 필수, `time`은 null 허용 | 날짜 없이 시간만 있는 경우("7시")는 `relative.hour`로 표현한다. `time`은 PRD 표기 `"HH:MM"`으로 직렬화하고, 읽을 때는 초 단위 표기도 받는다 |
| `relative`는 조합 가능한 선택 필드 묶음이며, 없는 필드는 직렬화하지 않는다 | PRD 5.1 예시가 필드를 생략한 형태다. 구조화 출력이 모든 필드를 null로 채워 보내는 경우도 받는다 |
| `relative` 필드: `day_offset`, `week_offset`, `weekday`, `month_offset`, `month`, `day`, `month_period`, `holiday{name, day_offset}`, `time_word`, `hour`, `minute` | PRD 5.3 표와 5.2 사전(담주/다담주, 모레/글피, 월말/월초, 상순/중순/하순)의 모든 표현을 담는다. 표현별 매핑 표는 `RelativeExpr` 문서 주석에 있다 |
| `weekday`는 `IsoWeekday`(1=월 ~ 7=일), 범위 밖이면 역직렬화 오류 | PRD 5.1 예시 `"weekday": 4` = 목요일. ISO-8601 (CLAUDE.md 규칙 3) |
| `time_word`·`month_period`의 JSON 값은 한국어(`"저녁"`, `"하순"`) | PRD 5.1 예시가 `"time_word": "저녁"`이다. 영어 메시지도 모델이 이 값으로 정규화한다 |
| `time_word` 값: 새벽·아침·오전·점심·오후·저녁·밤 | PRD에 나오는 것은 점심·저녁·오전·오후다. 새벽·아침·밤은 흔한 한국어 표현이라 넣었고, 이 셋의 기본 시각은 미결이다 |
| `AmbiguityReason`: `year_missing`, `am_pm_unknown`, `duration_unknown`, `multiple_candidates`, `explicit_relative_mismatch` | PRD 4.2의 네 가지와 5.1의 explicit/relative 불일치. `year_missing` 말고는 PRD에 식별자가 없어서 이번에 이름을 정했다 |
| `Ambiguity.alternatives`는 `Vec<String>` | PRD 5.1 예시를 따른다 (ISO 8601 문자열) |

### 4. 컨텍스트 (`core/src/context.rs`)

`ParseContext { reference, time_zone, working_hours, defaults, recent_locations }`는 PRD 5.2의 프롬프트 컨텍스트 목록을 그대로 옮긴 것이다. `TimeDefaults::default()`는 PRD 4.2 값(점심 12:00, 저녁 19:00, 오전 단독 10:00, 길이 60분)이다. `reference`는 메시지 수신 시각이 있으면 그 시각, 없으면 붙여넣기 시점이다.

### 5. 어댑터 트레이트 (`core/src/providers/mod.rs`)

- **동기 트레이트, `Send + Sync`.** 설정에 따라 공급자를 런타임에 바꿔야 하므로 `Box<dyn LlmProvider>`가 필요하다. 그런데 `async fn`을 쓰는 트레이트는 dyn으로 쓸 수 없다. `flutter_rust_bridge`는 동기 함수를 자체 스레드 풀에서 돌리고, XCFramework의 C ABI도 동기 함수가 단순하다. 0-4(FFI PoC)나 0-6(어댑터 구현)에서 맞지 않는다고 판단되면 바꾼다
- `LlmProvider::extract(text, context) -> ExtractionResult`. 재시도와 규칙 기반 폴백은 트레이트 밖에서 호출하는 쪽이 맡는다. 오류 메시지에 본문을 넣지 않는다 (PRD 9절)
- `CalendarProvider`의 메서드는 PRD 7절 6개를 그대로 따르고, 차이는 두 가지다:
  - `create_event`에 대상 `CalendarId` 인자를 더했다. 캘린더는 카드에서 고른다 (PRD 4.4)
  - 이벤트 식별자는 `EventRef { calendar, event }`다. Google API는 수정·삭제할 때 캘린더 ID를 같이 요구한다
- 확정된 일정은 별도 타입 `CalendarEvent`이고, LLM 중간 표현(`schema::Event`)과 섞지 않는다. 시간은 `EventTime::AllDay { start, end(마지막 날 포함) }` 또는 `EventTime::Timed { start, end, time_zone(IANA) }`로 나타낸다

### 6. 평가셋 형식 (`eval/`)

- 한 줄이 `{input, expected, reference_date, notes}`이고, 모르는 키는 오류로 처리한다
- `expected`는 LLM 중간 표현이 아니라 **후처리까지 끝난 확정값**이다. 시작·종료는 로컬 `YYYY-MM-DDTHH:MM`, 종일 일정은 `YYYY-MM-DD`로 적는다. 정확도는 사용자가 보게 될 최종 날짜·시간으로 재야 하기 때문이다
- 실행기는 지금 형식 검증만 한다. `--provider`를 주면 "미구현"으로 실패한다. CI는 `eval/ko-500.jsonl`(빈 파일)의 형식을 검사한다
- `eval/fixtures/prd-examples.jsonl`: PRD 3절 표에서 날짜가 확정된 3건 (기준일 2026-10-19 월요일)

## 대안

- 워크스페이스를 `core/` 안에 두고 `eval`은 별도 워크스페이스로: `cargo run -p eval`이 루트에서 동작하지 않아 기각
- `relative`를 표현 종류별 태그 enum(`{"kind": "next_week", ...}`)으로: 타입은 더 엄격해지지만 PRD 5.1 예시 형식과 달라지고, 소형 모델이 태그를 틀릴 여지가 커서 기각
- async 트레이트(`async-trait` 크레이트): FFI 경계가 정해지지 않은 상태에서 런타임 의존을 미리 들이는 셈이라 보류

## 미결 (PRD 확인 필요)

1. **연도 누락 날짜를 어느 필드로 낼지.** PRD 5.1 예시는 "10/24"를 `explicit.date = 2026-10-24`와 `year_missing` 모호성 플래그로 낸다. 즉 모델이 연도를 정한다. 반면 PRD 5.3과 CLAUDE.md 규칙 2는 "기준일 이후 가장 가까운 10/24"를 코드가 정한다고 한다. 스키마는 두 방식(`explicit` 또는 `relative {month, day}`)을 모두 받는다. 프롬프트가 어느 쪽을 요구할지는 0-3·0-6에서 정한다. 권장은 `relative`
2. **주 시작 요일과 주말 넘김.** CLAUDE.md 규칙 3은 "주 시작은 월요일"로 못 박았다. PRD 5.3은 "주 시작 요일 설정 (기본 월)"이고, "이번 주 금"은 "기준일이 토·일이면 다음 주로 넘김 (설정 가능)"이다. 문서끼리 충돌하므로 `ParseContext`에 설정 필드를 넣지 않았다. 0-3 전에 결정이 필요하다
3. **복수 후보("이번 주 금요일 or 다음 주 화요일").** PRD 3절은 "2개 후보 제시"라고만 한다. 처리 방법은 두 가지가 가능하다. (a) `multiple_candidates` 모호성의 `alternatives`에 넣는 방법인데, 문자열이므로 상대 표현을 담으려면 모델이 날짜를 계산해야 한다. (b) 이벤트를 2건 내고 카드가 하나를 고르게 하는 방법이다. 평가셋 `expected`에서 후보를 어떻게 적을지도 함께 미결이다
4. **알림 제안.** PRD 4.2 추출 항목에는 있지만 5.1 스키마에는 없다. 그래서 필드를 추가하지 않았다
5. **공휴일.** `holiday.name`에 넣을 이름 목록, "연휴"처럼 여러 날에 걸친 표현의 범위, 공휴일 데이터 출처가 정해지지 않았다
6. **월 구간.** 월초·상순·중순·하순·월말을 날짜 하나로 확정할지 범위로 둘지 정해지지 않았다
7. **기본 시각.** 새벽·아침·오후·밤 단독 표현의 기본 시각이 없다 (PRD 4.2에는 점심·저녁·오전만 있다)
8. **`LlmProvider` 부가 기능.** PRD 5.6의 모델 목록 조회(`/v1/models`, `/api/tags`)와 "연결 테스트"를 트레이트에 넣을지는 0-6에서 정한다
9. **`Calendar` 필드.** 계정, 색상, 쓰기 가능 여부 같은 필드가 PRD 7절에 없어서 지금은 `id`·`name`만 있다
10. **평가 채점.** 정확도를 어떻게 셀지(이벤트 단위 완전 일치인지 필드별인지, 제목·장소의 문자열 일치 기준)와, `reference_date`에 시각까지 넣어야 하는지("1시간 뒤" 같은 표현)가 정해지지 않았다
11. **시간대 라이브러리.** IANA 이름을 해석할 `chrono-tz` 등의 도입 여부는 0-3에서 정한다

## 결과

- `cargo test --workspace` 통과: core 스키마 9건, eval 형식 4건
- CI(`.github/workflows/ci.yml`): fmt, clippy(경고를 오류로 처리), test, 평가셋 형식 검증
- 다음 세션: 0-3(상대 날짜 해석기)은 미결 1·2·6·7번 결정을 전제로 한다. 0-6(어댑터)은 5번 결정과 미결 8번을 다룬다
