import assert from "node:assert/strict";
import { test } from "node:test";

import Anthropic from "@anthropic-ai/sdk";
import schema from "../../core/src/providers/extraction.schema.json" with { type: "json" };
import { DEFAULT_MODEL, claudeExtractor } from "../src/claude.ts";

const REQUEST = { instructions: "지시문", context: "컨텍스트", text: "내일 점심" };

function fakeClient(respond: (params: unknown) => unknown) {
  const calls: unknown[] = [];
  const client = {
    messages: {
      create: async (params: unknown) => {
        calls.push(params);
        return respond(params);
      },
    },
  };
  return { client: client as unknown as Anthropic, calls };
}

function message(stop_reason: string, content: unknown[]) {
  return { stop_reason, content };
}

test("요청 형식: 기본 모델, 캐시되는 지시문, 구조화 출력 스키마", async () => {
  const output = { events: [], no_event_found: true };
  const { client, calls } = fakeClient(() =>
    message("end_turn", [{ type: "thinking", thinking: "" }, { type: "text", text: JSON.stringify(output) }]),
  );
  const result = await claudeExtractor(client, DEFAULT_MODEL)(REQUEST);
  assert.deepEqual(result, { ok: true, output });

  const params = calls[0] as Record<string, any>;
  assert.equal(params.model, "claude-haiku-5-5");
  assert.deepEqual(params.system, [
    { type: "text", text: "지시문", cache_control: { type: "ephemeral" } },
    { type: "text", text: "컨텍스트" },
  ]);
  assert.deepEqual(params.messages, [{ role: "user", content: "내일 점심" }]);
  assert.deepEqual(params.output_config.format, { type: "json_schema", schema });
  assert.equal(params.output_config.effort, "low");
  assert.ok(params.max_tokens > 0);
});

test("거절·잘림·형식 오류", async () => {
  const cases: [unknown, string][] = [
    [message("refusal", []), "refusal"],
    [message("max_tokens", [{ type: "text", text: "{" }]), "max_tokens"],
    [message("end_turn", []), "invalid_output"],
    [message("end_turn", [{ type: "text", text: "일정이 없습니다" }]), "invalid_output"],
  ];
  for (const [response, reason] of cases) {
    const { client } = fakeClient(() => response);
    assert.deepEqual(await claudeExtractor(client, "m")(REQUEST), { ok: false, reason });
  }
});

test("API 오류", async () => {
  const rateLimited = Object.create(Anthropic.RateLimitError.prototype);
  const apiError = Object.create(Anthropic.InternalServerError.prototype);
  for (const [error, reason] of [
    [rateLimited, "upstream_rate_limited"],
    [apiError, "upstream_error"],
  ] as const) {
    const { client } = fakeClient(() => {
      throw error;
    });
    assert.deepEqual(await claudeExtractor(client, "m")(REQUEST), { ok: false, reason });
  }
  const { client } = fakeClient(() => {
    throw new TypeError("bug");
  });
  await assert.rejects(claudeExtractor(client, "m")(REQUEST), TypeError);
});
