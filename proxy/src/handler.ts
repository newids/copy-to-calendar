// 파싱 프록시 요청 처리 (PRD 8.2·9절, ADR 0004).
// 본문(메시지 원문·프롬프트)은 저장하지 않고 로그에도 남기지 않는다. 로그는 길이·지연·결과 코드만.

export interface ExtractRequest {
  /** 앱 버전마다 고정인 지시문. 프롬프트 캐시에 올린다. */
  instructions: string;
  /** 기준 일시·시간대 등 요청마다 바뀌는 컨텍스트. */
  context: string;
  /** 사용자가 붙여 넣은 메시지 원문. */
  text: string;
}

export type ExtractOutcome =
  | { ok: true; output: unknown }
  | {
      ok: false;
      reason: "refusal" | "max_tokens" | "invalid_output" | "upstream_rate_limited" | "upstream_error";
    };

export type Extractor = (request: ExtractRequest) => Promise<ExtractOutcome>;

export interface RateLimiter {
  allow(key: string, now: number): boolean;
}

export interface LogEntry {
  status: number;
  latency_ms: number;
  text_chars?: number;
  outcome?: string;
}

export interface Deps {
  extract: Extractor;
  limiter: RateLimiter;
  log: (entry: LogEntry) => void;
  now: () => number;
}

/** [__] 잠정값. 입력 길이 상한 (문자 수, ADR 0004). */
export const LIMITS = { instructions: 20_000, context: 2_000, text: 4_000, device: 128 } as const;

const STATUS: Record<Exclude<ExtractOutcome, { ok: true }>["reason"], number> = {
  refusal: 422,
  max_tokens: 502,
  invalid_output: 502,
  upstream_rate_limited: 503,
  upstream_error: 502,
};

function json(status: number, body: unknown): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { "Content-Type": "application/json; charset=utf-8" },
  });
}

function parseBody(raw: unknown): ExtractRequest | string {
  if (typeof raw !== "object" || raw === null) return "JSON 객체가 필요하다";
  const body = raw as Record<string, unknown>;
  for (const key of ["instructions", "context", "text"] as const) {
    const value = body[key];
    if (typeof value !== "string") return `${key}는 문자열이어야 한다`;
    if (value.length > LIMITS[key]) return `${key}가 너무 길다 (최대 ${LIMITS[key]}자)`;
  }
  if ((body.text as string).trim() === "") return "text가 비어 있다";
  return { instructions: body.instructions as string, context: body.context as string, text: body.text as string };
}

export async function handle(request: Request, deps: Deps): Promise<Response> {
  const started = deps.now();
  const done = (status: number, body: unknown, extra: Omit<LogEntry, "status" | "latency_ms"> = {}) => {
    deps.log({ status, latency_ms: deps.now() - started, ...extra });
    return json(status, body);
  };

  const url = new URL(request.url);
  if (url.pathname !== "/v1/extract") return done(404, { error: "not_found" });
  if (request.method !== "POST") return done(405, { error: "method_not_allowed" });

  const device = request.headers.get("X-C2C-Device");
  if (!device || device.length > LIMITS.device) return done(400, { error: "device_required" });
  if (!deps.limiter.allow(device, started)) return done(429, { error: "rate_limited" });

  let raw: unknown;
  try {
    raw = await request.json();
  } catch {
    return done(400, { error: "invalid_json" });
  }
  const parsed = parseBody(raw);
  if (typeof parsed === "string") return done(400, { error: "invalid_request", message: parsed });

  let outcome: ExtractOutcome;
  try {
    outcome = await deps.extract(parsed);
  } catch {
    outcome = { ok: false, reason: "upstream_error" };
  }
  const textChars = parsed.text.length;
  if (outcome.ok) return done(200, outcome.output, { text_chars: textChars, outcome: "ok" });
  return done(STATUS[outcome.reason], { error: outcome.reason }, { text_chars: textChars, outcome: outcome.reason });
}
