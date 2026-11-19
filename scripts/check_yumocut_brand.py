#!/usr/bin/env python3
"""Static checks for YumoCut's independent Windows identity."""

from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
OLD_IDS = (
    "7B1E5C3A-3B9E-4F0B-9C6D-2F1D0C0A0C47",
    "6C4D5D8E-9B2A-4F3B-8E1C-5A7D2B9F0C11",
)

for relative in ("assets/windows/concat.iss", "assets/windows/concat.wxs"):
    text = (ROOT / relative).read_text(encoding="utf-8")
    assert "YumoCut" in text, f"{relative}: missing YumoCut"
    for old_id in OLD_IDS:
        assert old_id not in text, f"{relative}: still uses upstream identity {old_id}"
    assert "yumocut.ico" in text, f"{relative}: missing YumoCut icon"

build = (ROOT / "src/crates/concat/build.rs").read_text(encoding="utf-8")
assert "yumocut.ico" in build, "build.rs: missing YumoCut icon"
assert 'set("ProductName", "YumoCut")' in build
assert 'set("FileDescription", "YumoCut")' in build

for relative in ("assets/icons/yumocut.svg", "assets/icons/yumocut.ico"):
    path = ROOT / relative
    assert path.is_file() and path.stat().st_size > 0, f"{relative}: missing or empty"

print("YumoCut brand checks passed")
