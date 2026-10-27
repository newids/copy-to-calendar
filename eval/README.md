# eval — 한국어 일정 메시지 평가셋·회귀
ko-500.jsonl: {input, expected, reference_date, notes}. 주 1회 CI 회귀, 목표 정확도 95%. (PRD 5.5·5.7절)

## 형식

한 줄에 한 케이스. 모르는 키는 오류(`deny_unknown_fields`).

```json
{"input": "담주 목욜 저녁 7시 강남역 2번출구 앞에서 봐요",
 "expected": {"events": [{"start": "2026-10-29T19:00", "end": "2026-10-29T20:00", "location": "강남역 2번 출구"}]},
 "reference_date": "2026-10-19",
 "notes": "PRD 3절 예시"}
```

- `expected`는 후처리까지 끝난 **확정값**이다 (LLM 중간 표현 아님)
  - `events[]`: `title`·`end`·`location`·`recurrence`는 생략 가능(null), `all_day` 기본 false
  - `start`·`end`: 종일이면 `YYYY-MM-DD`, 아니면 로컬 `YYYY-MM-DDTHH:MM` (시간대 없음)
  - 서로 대안인 후보("이번 주 금 or 다음 주 화")는 이벤트를 따로 적고 같은 `candidate_group` 번호를 준다
  - 일정이 없으면 `{"no_event_found": true}`
- `reference_date`: 상대 표현을 해석할 기준일
- 예시: `fixtures/prd-examples.jsonl` (PRD 3절 표 중 날짜가 확정된 3건)

## 실행

```
cargo run -p eval -- --set eval/ko-500.jsonl                   # 형식 검증 (CI)
cargo run -p eval -- --set <set.jsonl> --outputs <outputs.jsonl> --min-accuracy 0.95
cargo run -p eval -- --set eval/ko-500.jsonl --provider <name>  # 0-6 이후: 모델 출력 기록용
```

- `--outputs`: 케이스와 같은 줄 순서로 모델 출력(PRD 5.1 JSON)을 담은 JSONL
  - core로 조립(ADR 0003)한 결과를 `expected`와 비교해 정확도를 낸다
  - `--min-accuracy`보다 낮으면 실패로 끝난다
- 채점: 일정 수, 각 일정의 시작·종료(`expected`에 있을 때)·종일 여부·후보 묶음이 모두 맞아야 정답이다
  - 일정 순서는 보지 않는다
  - 제목·장소는 채점하지 않는다
- 모델 출력은 기록해서 고정한다. 채점은 항상 기록된 출력 파일로 하고, `--provider`는 출력을 새로 기록할 때만 쓴다 (ADR 0003)
- CI는 `fixtures/prd-examples.jsonl`과 `fixtures/prd-examples.outputs.jsonl`을 정확도 100%로 돌린다
  - 출력 fixture는 사람이 쓴 것이고, 실제 모델 출력이 아니다
