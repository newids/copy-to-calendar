// 기기별 고정 창 레이트 리밋. Worker 인스턴스(isolate)마다 따로 세는 최선 노력 방식이다.
// 여러 인스턴스에 걸친 정확한 제한은 운영 전에 정한다 (ADR 0004 미결).

import type { RateLimiter } from "./handler.ts";

const WINDOW_MS = 60_000;
/** 메모리 상한. 넘으면 오래된 창을 비운다. */
const MAX_KEYS = 10_000;

export class FixedWindowLimiter implements RateLimiter {
  private readonly counts = new Map<string, { window: number; count: number }>();
  private readonly perMinute: number;

  constructor(perMinute: number) {
    this.perMinute = perMinute;
  }

  allow(key: string, now: number): boolean {
    const window = Math.floor(now / WINDOW_MS);
    const entry = this.counts.get(key);
    if (!entry || entry.window !== window) {
      if (this.counts.size >= MAX_KEYS) this.evict(window);
      this.counts.set(key, { window, count: 1 });
      return this.perMinute > 0;
    }
    entry.count += 1;
    return entry.count <= this.perMinute;
  }

  private evict(window: number) {
    for (const [key, entry] of this.counts) {
      if (entry.window !== window) this.counts.delete(key);
    }
    if (this.counts.size >= MAX_KEYS) this.counts.clear();
  }
}
