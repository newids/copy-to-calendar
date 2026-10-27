// C2C 파싱 프록시 (Cloudflare Workers). 계약은 proxy/README.md.

import Anthropic from "@anthropic-ai/sdk";
import { DEFAULT_MODEL, claudeExtractor } from "./claude.ts";
import { handle } from "./handler.ts";
import { FixedWindowLimiter } from "./rate-limit.ts";

export interface Env {
  ANTHROPIC_API_KEY: string;
  MODEL?: string;
  RATE_LIMIT_PER_MINUTE?: string;
}

let limiter: FixedWindowLimiter | undefined;

export default {
  async fetch(request: Request, env: Env): Promise<Response> {
    limiter ??= new FixedWindowLimiter(Number(env.RATE_LIMIT_PER_MINUTE ?? "0"));
    const client = new Anthropic({ apiKey: env.ANTHROPIC_API_KEY });
    return handle(request, {
      extract: claudeExtractor(client, env.MODEL ?? DEFAULT_MODEL),
      limiter,
      log: (entry) => console.log(JSON.stringify(entry)),
      now: Date.now,
    });
  },
} satisfies ExportedHandler<Env>;
