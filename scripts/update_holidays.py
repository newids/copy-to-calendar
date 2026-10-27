#!/usr/bin/env python3
"""공휴일 테이블 갱신 (PRD 5.3, ADR 0001 8절·0002).

한국천문연구원 특일 정보 API(공공데이터포털, SpcdeInfoService/getRestDeInfo)에서 연도별 공휴일을
받아 core/data/holidays.json에 합친다. 지정한 연도의 기존 항목은 새 응답으로 바꾼다.

    DATA_GO_KR_SERVICE_KEY=... python3 scripts/update_holidays.py 2026 2027

서비스 키는 공공데이터포털에서 발급받은 일반 인증키(Decoding)다. 앱에는 넣지 않는다.
"""

import argparse
import json
import os
import sys
import urllib.parse
import urllib.request
from pathlib import Path

ENDPOINT = "https://apis.data.go.kr/B090041/openapi/service/SpcdeInfoService/getRestDeInfo"
OUTPUT = Path(__file__).resolve().parent.parent / "core" / "data" / "holidays.json"


def fetch(year: int, key: str) -> dict:
    query = urllib.parse.urlencode(
        {"ServiceKey": key, "solYear": year, "numOfRows": 100, "_type": "json"}
    )
    with urllib.request.urlopen(f"{ENDPOINT}?{query}", timeout=30) as response:
        return json.load(response)


def parse(payload: dict) -> list[dict]:
    """응답에서 {date, name} 목록을 뽑는다. item이 1건이면 객체, 0건이면 빈 문자열로 온다."""
    items = payload["response"]["body"]["items"]
    if not items:
        return []
    item = items["item"]
    rows = item if isinstance(item, list) else [item]
    holidays = []
    for row in rows:
        if row.get("isHoliday") != "Y":
            continue
        d = str(row["locdate"])
        holidays.append({"date": f"{d[:4]}-{d[4:6]}-{d[6:]}", "name": row["dateName"]})
    return holidays


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("years", nargs="+", type=int)
    args = parser.parse_args()

    key = os.environ.get("DATA_GO_KR_SERVICE_KEY")
    if not key:
        print("DATA_GO_KR_SERVICE_KEY가 필요하다", file=sys.stderr)
        return 2

    existing = json.loads(OUTPUT.read_text(encoding="utf-8")) if OUTPUT.exists() else []
    years = {str(y) for y in args.years}
    merged = [h for h in existing if h["date"][:4] not in years]
    for year in args.years:
        rows = parse(fetch(year, key))
        print(f"{year}: {len(rows)}건")
        merged.extend(rows)

    merged.sort(key=lambda h: (h["date"], h["name"]))
    OUTPUT.write_text(json.dumps(merged, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    return 0


if __name__ == "__main__":
    sys.exit(main())
