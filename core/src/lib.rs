//! `c2c-core` — AI Copy To Calendar(C2C) 공통 코어.
//!
//! LLM은 의미 해석만 하고, 날짜 확정·시간대·검증은 이 크레이트의 결정적 코드가 맡는다 (PRD 5절).
//! LLM 어댑터 구현은 0-6에서 추가한다.

pub mod calendar;
pub mod context;
pub mod holidays;
pub mod providers;
pub mod relative;
pub mod schema;
