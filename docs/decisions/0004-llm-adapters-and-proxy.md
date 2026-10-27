# 0004. LLM 어댑터 3종과 파싱 프록시

- 상태: 채택 (0-6 세션, 2026-10-27)
- 관련: PRD 5.1·5.2·5.5·5.6·5.7·8.2·9절, ADR 0001 5절·미결 2번, ADR 0003
- 코드: `core/src/providers/`, `proxy/`, `eval/src/record.rs`

## 배경

HANDOFF 0-6의 산출물은 세 가지다: 프록시 골격, 클라우드·Ollama·OpenAI 호환 어댑터 3종, 샘플 1건의 JSON 유효성 확인. 모든 공급자가 PRD 5.1 스키마를 내야 하고, 날짜 계산은 하지 않아야 한다 (CLAUDE.md 규칙 2).

## 결정

### 1. 프롬프트와 출력 스키마는 core 한 곳에 둔다

- `core/src/providers/prompt.rs`
  - 지시문: 날짜 계산 금지, `relative` 필드 사전, 공휴일 이름, few-shot 6건
  - 요청 컨텍스트: 기준 일시와 요일, 시간대, 근무시간, 기본 시각, 최근 장소 10개 (PRD 5.2)
  - 공휴일 이름은 내장 테이블에서 뽑는다. 대체공휴일은 뺀다
- 지시문은 앱 버전마다 고정이다. 그래서 앞에 두고 프롬프트 캐시 대상으로 삼는다. 요청마다 바뀌는 컨텍스트는 그 뒤에 둔다
- `core/src/providers/extraction.schema.json`: 구조화 출력 strict 모드 형식이다
  - 모든 속성이 required이고, 값이 없는 필드는 null을 허용한다. `additionalProperties: false`
  - 모델이 낼 수 있는 모호성은 `am_pm_unknown`·`duration_unknown`뿐이다. 나머지는 코드가 판단한다 (ADR 0001 7절)
- 같은 스키마 파일을 세 어댑터와 프록시가 함께 쓴다
- 테스트 두 가지로 일관성을 지킨다
  - 스키마 속성과 serde 타입 필드가 양방향으로 같은지
  - few-shot 출력 예시가 스키마 타입으로 읽히는지

### 2. 어댑터 (`core/src/providers/`)

| 어댑터 | 호출 | 스키마 강제 |
| --- | --- | --- |
| `CloudProvider` | `POST {base}/v1/extract` + `X-C2C-Device` | 프록시가 강제 |
| `OpenAiCompatProvider` | `POST {base}/chat/completions` | `response_format: {type: "json_schema", json_schema: {strict: true, schema}}` |
| `OllamaProvider` | `POST {base}/api/chat` (`stream: false`) | `format: <schema>` (PRD 5.6: Ollama의 OpenAI 호환 엔드포인트는 json_schema를 무시) |

- HTTP는 `ureq`(동기)로 한다. ADR 0001 5절의 동기 트레이트에 맞춘 선택이다
- 타임아웃은 호출하는 쪽이 정한다. eval 기본값은 PRD 5.6의 로컬 기본값과 같은 30초다
- 오류는 이렇게 바꾼다
  - HTTP 429 → `RateLimited`
  - 그 밖의 HTTP 오류 → `Transport`
  - 응답이 JSON이 아니거나 스키마와 다름 → `InvalidOutput`
  - 시간 초과 → `Timeout`
- 오류 메시지에 메시지 본문을 넣지 않는다 (PRD 9절)
- `extract_with_retry`: `InvalidOutput`이면 같은 요청을 한 번 더 보낸다 (PRD 5.1·5.6). 그래도 실패하면 오류를 돌려주고, 수동 입력이나 규칙 기반 폴백은 앱이 맡는다
- **ADR 0001 미결 2번 결정**
  - 모델 목록 조회(`list_models`: OpenAI 호환은 `/models`, Ollama는 `/api/tags`)는 로컬 어댑터의 고유 메서드로 둔다. 트레이트에 넣지 않은 이유는 클라우드에는 해당하지 않아서다
  - "연결 테스트"는 앱이 샘플 1건으로 `extract`를 호출해 응답 시간과 JSON 유효성을 보는 방식으로 한다

### 3. 파싱 프록시 (`proxy/`)

- **얇은 중계로 만든다.** 앱이 `{instructions, context, text}`를 보내면, 프록시는 아래를 고정한 채 Claude를 호출한다
  - 모델: `MODEL`, 기본 `claude-haiku-5-5` (PRD 5.7 1순위)
  - 출력 스키마
  - `max_tokens`
- 프롬프트를 core에 한 벌만 두려고 이 방식을 골랐다. 그 대신 프록시를 범용 LLM으로 악용할 여지가 생긴다. 이것은 스키마로 고정한 출력, 토큰 상한, 레이트 리밋으로 막는다
- Anthropic 공식 TypeScript SDK(`@anthropic-ai/sdk` 0.133.0)로 호출한다
  - `messages.create`에 `output_config.format: {type: "json_schema", schema}`(구조화 출력)를 쓴다
  - `system`은 [지시문(`cache_control: ephemeral`), 컨텍스트] 두 블록이다
  - `output_config.effort`는 `low`다. 단순 추출이고 PRD의 응답 시간 목표 때문이다. 평가셋 결과로 조정한다
  - `stop_reason`이 `refusal`이면 422, `max_tokens`면 502로 응답한다. 텍스트 블록이 JSON이 아니면 502다
- 레이트 리밋
  - 기기 ID 헤더별 분당 고정 창으로 센다
  - Worker 인스턴스마다 따로 세는 최선 노력 방식이다
  - 상한 `RATE_LIMIT_PER_MINUTE`은 잠정값이다
- 요청 서명 검증은 넣지 않았다. PRD 9절에 따르면 v1.0은 레이트 리밋만 하고, v1.5에서 App Attest·Play Integrity를 붙인다. `.env.example`의 `PROXY_SIGNING_SECRET`은 아직 쓰지 않는다
- 로그는 상태·지연·입력 길이·결과 코드만 남긴다. 본문, 프롬프트, 기기 ID는 남기지 않으며, 테스트로 확인한다 (CLAUDE.md 규칙 4)
- 테스트는 `node:test`와 Node 22의 TypeScript 타입 스트리핑으로 돌린다
  - vitest를 쓰지 않은 이유: vitest 4·5 모두 npm의 optional peer 의존성 해석 오류(`Cannot read properties of null (reading 'edgesOut')`)로 설치가 되지 않았다
  - 개발 의존성은 TypeScript와 Workers 타입뿐이다
- `wrangler deploy --dry-run`으로 번들이 만들어지는 것(678 KiB)까지 확인했다. 실제 배포는 하지 않았다
- 의존성은 공개된 지 2주 이상 지난 버전으로 고정했다

### 4. eval 기록 (ADR 0003 3절의 실행)

- `eval --provider <openai-compat|ollama|cloud> --base-url … --model … --record <파일>`은 케이스마다 `extract_with_retry`를 호출해 출력을 한 줄씩 기록하고, 이어서 채점한다
- 실패한 케이스는 `{"error": "..."}`로 기록하고 오답으로 센다. 빈 결과로 읽히지 않게 막아 두었다. 그렇지 않으면 "일정 없음" 케이스를 맞힌 것으로 셀 수 있다
- 로컬 목 Ollama 서버로 기록 → 채점이 끝까지 돌아가는 것을 확인했다 (PRD 예시 3건 100%)

## 대안

- 프록시가 `{text, context}`만 받고 프롬프트를 직접 만들기: 남용 여지는 줄지만 프롬프트가 Rust와 TypeScript 두 벌이 되어 기각
- 프록시에서 강제 tool use로 JSON 받기: Claude Haiku 5.5 계열은 구조화 출력을 지원하고, 최신 모델에서는 강제 `tool_choice`가 400이라서 기각
- 비동기 HTTP(`reqwest` + tokio): 트레이트가 동기라서 기각 (ADR 0001 5절)

## 미결

1. **HANDOFF 게이트 "샘플 1건 JSON 유효" 실측.** 이 클라우드 세션에는 API 키도 로컬 LLM도 없어서, 목 서버로 형식만 검증했다. 아래 중 하나로 실제 모델을 확인해야 한다
   - 로컬에서 Ollama나 LM Studio에 `eval --provider … --record`를 실행한다
   - `ANTHROPIC_API_KEY`를 넣어 프록시를 `wrangler dev`로 띄우고 `--provider cloud`로 실행한다
2. **레이트 리밋 운영 방식.** 상한값(`RATE_LIMIT_PER_MINUTE`, 지금은 잠정 20)을 정해야 한다. 여러 인스턴스에 걸친 정확한 제한을 무엇으로 할지(Cloudflare 레이트 리밋 기능, Durable Object 등)도 정해야 한다
3. **입력 길이 상한.** `proxy/src/handler.ts`의 `LIMITS`(지시문 20,000자, 컨텍스트 2,000자, 본문 4,000자)는 잠정값이다
4. **effort와 최종 모델.** 평가셋 결과로 정한다 (PRD 13절 "LLM 1차 공급자 선정")
5. **2순위 공급자 라우팅.** PRD 5.5·12절에 따르면 공급자를 2곳 이상 통합해야 하지만, 지금 프록시는 Claude만 부른다
6. **프롬프트 캐시 효과.** 지시문이 모델의 최소 캐시 길이를 넘는지 실호출의 `usage.cache_read_input_tokens`로 확인해야 한다

## 결과

- core 어댑터 테스트 10건(`core/tests/providers.rs`, 로컬 목 서버), eval 기록 테스트 1건, 프록시 테스트 10건
- CI에 `proxy` 작업(typecheck, test)을 추가했다
