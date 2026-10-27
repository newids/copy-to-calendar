import assert from "node:assert/strict";
import { test } from "node:test";

import { FixedWindowLimiter } from "../src/rate-limit.ts";

test("기기별 분당 상한", () => {
  const limiter = new FixedWindowLimiter(2);
  assert.equal(limiter.allow("a", 0), true);
  assert.equal(limiter.allow("a", 1_000), true);
  assert.equal(limiter.allow("a", 2_000), false);
  assert.equal(limiter.allow("b", 2_000), true, "다른 기기는 따로 센다");
  assert.equal(limiter.allow("a", 60_000), true, "다음 창에서 다시 허용");
});

test("상한 0이면 모두 막는다", () => {
  assert.equal(new FixedWindowLimiter(0).allow("a", 0), false);
});
