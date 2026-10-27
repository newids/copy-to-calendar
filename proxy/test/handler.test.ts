import assert from "node:assert/strict";
import { test } from "node:test";

import { type Deps, type ExtractOutcome, type ExtractRequest, type LogEntry, LIMITS, handle } from "../src/handler.ts";

const BODY = { instructions: "지시문", context: "기준 일시: 2026-10-19", text: "비밀 메시지 내일 점심" };
const OUTPUT = { events: [], no_event_found: true };

function setup(outcome: ExtractOutcome | Error = { ok: true, output: OUTPUT }, allow = true) {
  const logs: LogEntry[] = [];
  const calls: ExtractRequest[] = [];
  let clock = 1_000;
  const deps: Deps = {
    extract: async (req) => {
      calls.push(req);
      clock += 250;
      if (outcome instanceof Error) throw outcome;
      return outcome;
    },
    limiter: { allow: () => allow },
    log: (e) => logs.push(e),
    now: () => clock,
  };
  return { deps, logs, calls };
}

function request(body: unknown = BODY, init: { method?: string; path?: string; device?: string | null } = {}) {
  const headers = new Headers({ "Content-Type": "application/json" });
  if (init.device !== null) headers.set("X-C2C-Device", init.device ?? "device-1");
  return new Request(`https://proxy.test${init.path ?? "/v1/extract"}`, {
    method: init.method ?? "POST",
    headers,
    body: init.method === "GET" ? undefined : typeof body === "string" ? body : JSON.stringify(body),
  });
}

test("성공하면 모델 출력을 그대로 돌려준다", async () => {
  const { deps, logs, calls } = setup();
  const res = await handle(request(), deps);
  assert.equal(res.status, 200);
  assert.deepEqual(await res.json(), OUTPUT);
  assert.deepEqual(calls, [BODY]);
  assert.deepEqual(logs, [{ status: 200, latency_ms: 250, text_chars: BODY.text.length, outcome: "ok" }]);
});

test("로그에 본문을 남기지 않는다", async () => {
  for (const outcome of [{ ok: true, output: OUTPUT } as const, { ok: false, reason: "refusal" } as const, new Error("boom")]) {
    const { deps, logs } = setup(outcome);
    await handle(request(), deps);
    const logged = JSON.stringify(logs);
    for (const part of Object.values(BODY)) assert.ok(!logged.includes(part), `${part} 이(가) 로그에 있음`);
    assert.ok(!logged.includes("device-1"));
  }
});

test("요청 검증", async () => {
  const cases: [Request, number, string][] = [
    [request(BODY, { path: "/v1/other" }), 404, "not_found"],
    [request(BODY, { method: "GET" }), 405, "method_not_allowed"],
    [request(BODY, { device: null }), 400, "device_required"],
    [request(BODY, { device: "x".repeat(LIMITS.device + 1) }), 400, "device_required"],
    [request("{not json"), 400, "invalid_json"],
    [request([1, 2]), 400, "invalid_request"],
    [request({ ...BODY, text: 3 }), 400, "invalid_request"],
    [request({ ...BODY, text: "  " }), 400, "invalid_request"],
    [request({ ...BODY, text: "가".repeat(LIMITS.text + 1) }), 400, "invalid_request"],
    [request({ ...BODY, instructions: "가".repeat(LIMITS.instructions + 1) }), 400, "invalid_request"],
  ];
  for (const [req, status, error] of cases) {
    const { deps, calls } = setup();
    const res = await handle(req, deps);
    assert.equal(res.status, status, error);
    assert.equal(((await res.json()) as { error: string }).error, error);
    assert.equal(calls.length, 0, "검증 실패면 모델을 부르지 않는다");
  }
});

test("레이트 리밋", async () => {
  const { deps, calls } = setup(undefined, false);
  const res = await handle(request(), deps);
  assert.equal(res.status, 429);
  assert.equal(calls.length, 0);
});

test("모델 실패를 상태 코드로 바꾼다", async () => {
  const cases: [ExtractOutcome | Error, number, string][] = [
    [{ ok: false, reason: "refusal" }, 422, "refusal"],
    [{ ok: false, reason: "max_tokens" }, 502, "max_tokens"],
    [{ ok: false, reason: "invalid_output" }, 502, "invalid_output"],
    [{ ok: false, reason: "upstream_rate_limited" }, 503, "upstream_rate_limited"],
    [{ ok: false, reason: "upstream_error" }, 502, "upstream_error"],
    [new Error("network"), 502, "upstream_error"],
  ];
  for (const [outcome, status, error] of cases) {
    const { deps, logs } = setup(outcome);
    const res = await handle(request(), deps);
    assert.equal(res.status, status, error);
    assert.deepEqual(await res.json(), { error });
    assert.equal(logs[0].outcome, error);
  }
});
