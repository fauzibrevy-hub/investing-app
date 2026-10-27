"""Regenerate tests/fixtures/thesis_golden/*.json by running MC's own
thesis_bank parsers over the synthetic vault in tests/fixtures/vault.

Dev-only. Needs a Mission Control checkout and its Python env, e.g.:

  PYTHONDONTWRITEBYTECODE=1 <MC>/backend/venv/bin/python \
      scripts/golden/thesis_golden.py <MC> tests/fixtures/vault tests/fixtures/thesis_golden

Read-only towards MC: nothing is written into the MC checkout.
"""
import json
import sys
import tempfile
from pathlib import Path

mc, vault, out = (Path(a).resolve() for a in sys.argv[1:4])
sys.path.insert(0, str(mc))

from backend.api import thesis_bank as tb  # noqa: E402

fin = vault / "finance"
tb.STOCK_ROOT = fin / "stock"
tb.ARTICLE_BANK = fin / "article-bank"
tb.VAULT_THESIS_DIR = fin / "thesis"
tb.THESIS_DRAFTS_PATH = Path(tempfile.mkdtemp()) / "thesis_drafts.json"

out.mkdir(parents=True, exist_ok=True)


def dump(name, payload):
    # FastAPI serializes with jsonable_encoder; dict keys become strings.
    text = json.dumps(payload, ensure_ascii=False, sort_keys=True, indent=1)
    (out / f"{name}.json").write_text(text + "\n", encoding="utf-8")


summary = tb._build_summary()
dump("summary", summary)
for issuer in summary["issuers"]:
    t = issuer["ticker"]
    dump(f"detail_{t}", tb._public_thesis_detail(tb._build_detail(t)))
dump("macro", tb._build_macro())
dump("drafts_vault", tb.list_vault_theses())
print("wrote", sorted(p.name for p in out.iterdir()))
