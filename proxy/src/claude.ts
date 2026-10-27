// Claude 호출 (PRD 5.7: 1순위 Claude Haiku 5.5). 출력 스키마는 구조화 출력(output_config.format)으로 강제한다.
// 스키마는 core와 같은 파일을 쓴다.

import Anthropic from "@anthropic-ai/sdk";
import schema from "../../core/src/providers/extraction.schema.json" with { type: "json" };
import type { Extractor } from "./handler.ts";

export const DEFAULT_MODEL = "claude-haiku-5-5";

/** 추출 결과는 수백 토큰이다. 상한은 비정상 출력과 남용에 대한 방어. */
const MAX_TOKENS = 4096;

export function claudeExtractor(client: Anthropic, model: string): Extractor {
  return async ({ instructions, context, text }) => {
    let response: Anthropic.Message;
    try {
      response = await client.messages.create({
        model,
        max_tokens: MAX_TOKENS,
        system: [
          { type: "text", text: instructions, cache_control: { type: "ephemeral" } },
          { type: "text", text: context },
        ],
        messages: [{ role: "user", content: text }],
        // 단순 추출이라 effort는 낮게 둔다 (PRD 2절: 등록까지 5초 이하). 평가셋 결과로 조정한다.
        output_config: { effort: "low", format: { type: "json_schema", schema } },
      });
    } catch (e) {
      if (e instanceof Anthropic.RateLimitError) return { ok: false, reason: "upstream_rate_limited" };
      if (e instanceof Anthropic.APIError) return { ok: false, reason: "upstream_error" };
      throw e;
    }

    if (response.stop_reason === "refusal") return { ok: false, reason: "refusal" };
    if (response.stop_reason === "max_tokens") return { ok: false, reason: "max_tokens" };
    const block = response.content.find((b) => b.type === "text");
    if (!block || block.type !== "text") return { ok: false, reason: "invalid_output" };
    try {
      return { ok: true, output: JSON.parse(block.text) };
    } catch {
      return { ok: false, reason: "invalid_output" };
    }
  };
}
