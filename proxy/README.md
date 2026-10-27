# proxy — 파싱 프록시 (Cloudflare Workers)
LLM 키 보호, 레이트 리밋, 모델 라우팅. 본문 미저장, 로그에 본문 제외. (PRD 8.2·9절, ADR 0004)

## 계약

`POST /v1/extract`

- 헤더 `X-C2C-Device`: 기기별 익명 ID (최대 128자). 레이트 리밋 키다
- 본문 `{"instructions": string, "context": string, "text": string}`
  - `core/src/providers/prompt.rs`의 `Prompt`를 그대로 보낸다
  - 길이 상한: `src/handler.ts`의 `LIMITS`
- 응답 200: PRD 5.1 추출 결과 JSON
- 실패하면 `{"error": "<code>"}`로 응답한다

| 상태 | `error` | 뜻 |
| --- | --- | --- |
| 400 | `device_required`, `invalid_json`, `invalid_request` | 요청 형식 오류 |
| 404 / 405 | `not_found`, `method_not_allowed` | 경로·메서드 오류 |
| 422 | `refusal` | 모델이 거절 |
| 429 | `rate_limited` | 기기당 분당 상한 초과 |
| 502 | `max_tokens`, `invalid_output`, `upstream_error` | 모델 응답 이상 또는 API 오류 |
| 503 | `upstream_rate_limited` | Anthropic API 한도 초과 |

- 모델: `MODEL` (기본 `claude-haiku-5-5`, PRD 5.7)
- 출력 스키마는 `core/src/providers/extraction.schema.json`을 그대로 써서 구조화 출력으로 강제한다
- 로그에는 상태·지연·입력 길이·결과 코드만 남긴다. 본문과 기기 ID는 남기지 않는다

## 개발

```
npm ci
npm test            # node:test (Node 22.18+, TypeScript 타입 스트리핑)
npm run typecheck
npx wrangler@4.149.0 dev                         # 로컬 실행 (.dev.vars에 ANTHROPIC_API_KEY)
npx wrangler@4.149.0 secret put ANTHROPIC_API_KEY
npx wrangler@4.149.0 deploy
```
