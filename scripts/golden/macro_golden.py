"""Regenerate tests/fixtures/macro/golden/*.json by running MC's own macro
ingestion + scoring over synthetic inputs:

- tests/fixtures/macro/store.json     a 15-metric persistent store
- tests/fixtures/macro/upstream.json  canned upstream responses by URL

Dev-only. Needs a Mission Control checkout and an interpreter with its deps:

  PYTHONDONTWRITEBYTECODE=1 /usr/local/bin/python3 scripts/golden/macro_golden.py \
      <MC> tests/fixtures/macro

Time is frozen at 2026-10-20T08:00:00.123456Z. Nothing is written into MC:
the persistent-store path is redirected to a temp file.
"""
import asyncio
import copy
import json
import sys
import tempfile
from datetime import date, datetime, timezone
from pathlib import Path

mc, fixtures = (Path(a).resolve() for a in sys.argv[1:3])
sys.path.insert(0, str(mc))

import httpx  # noqa: E402

from backend.services import macro_ingestion as mi  # noqa: E402
from backend.services import macro_scoring as ms  # noqa: E402

NOW = datetime(2026, 10, 20, 8, 0, 0, 123456, tzinfo=timezone.utc)


class FrozenDT(datetime):
    @classmethod
    def now(cls, tz=None):
        return NOW if tz else NOW.replace(tzinfo=None)


mi.datetime = FrozenDT
ms.datetime = FrozenDT
mi.DATA_STORE_PATH = Path(tempfile.mkdtemp()) / "macro_store.json"

store = json.loads((fixtures / "store.json").read_text(encoding="utf-8"))
upstream = json.loads((fixtures / "upstream.json").read_text(encoding="utf-8"))


def handler(request: httpx.Request) -> httpx.Response:
    spec = upstream.get(str(request.url))
    if spec is None:
        raise httpx.ConnectError("All connection attempts failed", request=request)
    ext = {"reason_phrase": spec["reason"].encode()} if "reason" in spec else {}
    return httpx.Response(spec["status"], headers=spec["headers"], content=spec["body"].encode(), extensions=ext)


_orig_init = httpx.AsyncClient.__init__


def _init(self, *a, **k):
    k["transport"] = httpx.MockTransport(handler)
    _orig_init(self, *a, **k)


httpx.AsyncClient.__init__ = _init

out = fixtures / "golden"
out.mkdir(exist_ok=True)


def dump(name, payload):
    text = json.dumps(payload, ensure_ascii=False, sort_keys=True, indent=1)
    (out / f"{name}.json").write_text(text + "\n", encoding="utf-8")


today = date(2026, 10, 20)


def observations(st, isolated):
    return [mi.build_observation_from_store(r, today, is_fred_isolated=isolated, store=st) for r in mi.CANONICAL_REGISTRY]


def scoring(obs, disabled):
    d = ms.calculate_macro_scoring(obs, disabled_metric_ids=set(disabled)).model_dump()
    d["blocking_feeds"] = sorted(d["blocking_feeds"])
    return d


for isolated in (False, True):
    dump(f"observations_isolated_{str(isolated).lower()}", [o.model_dump() for o in observations(store, isolated)])
obs = observations(store, False)
for name, disabled in {"default": [], "vix_cut": ["vix"], "hy_cut": ["us_hy_oas"], "brent_curve_cut": ["brent_curve"],
                       "two_cuts": ["ust_10y", "dxy"]}.items():
    dump(f"scoring_{name}", scoring(obs, disabled))
dump("scoring_isolated", scoring(observations(store, True), []))
dump("scoring_empty_store", scoring(observations({}, False), []))


async def ingest(disable_fred):
    mi._PERSISTENT_STORE.clear()
    mi._PERSISTENT_STORE.update(copy.deepcopy(store))
    results = await asyncio.gather(*(mi.execute_upstream_ingestion(r, disable_fred=disable_fred) for r in mi.CANONICAL_REGISTRY))
    changed = [r.metric_id for r, (_, is_new) in zip(mi.CANONICAL_REGISTRY, results) if is_new]
    return {"store": copy.deepcopy(mi._PERSISTENT_STORE), "changed": changed}


for disable_fred in (False, True):
    dump(f"ingest_disable_fred_{str(disable_fred).lower()}", asyncio.run(ingest(disable_fred)))
print("wrote", sorted(p.name for p in out.iterdir()))
