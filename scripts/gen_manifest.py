"""Record SHA-256 in canonical docs/mods.yaml; explicit network maintenance only.

Run: python scripts/gen_manifest.py [--refresh]
Install dependency: python -m pip install -r scripts/requirements.txt
Existing valid hashes are retained unless --refresh is explicitly requested.
All downloads finish before atomic replacement; user_mods and other fields survive.
"""
import hashlib
import re
import sys
import urllib.request
from pathlib import Path
import yaml

REPO = Path(__file__).resolve().parent.parent
SRC = REPO / "docs" / "mods.yaml"
OUT = SRC
CHUNK = 1 << 20


def sha256(url, timeout=120):
    req = urllib.request.Request(url, headers={"User-Agent": "currspice-mod-installer/0.1"})
    h = hashlib.sha256()
    done = 0
    with urllib.request.urlopen(req, timeout=timeout) as response:
        while chunk := response.read(CHUNK):
            h.update(chunk)
            done += len(chunk)
    if not done:
        raise ValueError(f"Empty download: {url}")
    return h.hexdigest(), done


def main(refresh=False, check=False):
    if refresh and check:
        raise ValueError("--check and --refresh are mutually exclusive")
    doc = yaml.safe_load(SRC.read_text(encoding="utf-8"))
    if not isinstance(doc, dict) or not all(doc.get(k) for k in ("mc_version", "installer_version", "fabric", "server_mods")):
        raise ValueError("Missing required manifest fields")
    for group in ("server_mods", "user_mods"):
        entries = doc.get(group) or []
        output = []
        for entry in entries:
            item = {"url": entry} if isinstance(entry, str) else dict(entry)
            url = item.get("url", "")
            if not url.startswith("https://"):
                raise ValueError(f"Expected HTTPS mod URL: {url}")
            existing = item.get("sha256")
            if existing is not None and (not isinstance(existing, str) or not re.fullmatch(r"[0-9a-fA-F]{64}", existing)):
                raise ValueError(f"Invalid recorded SHA-256: {url}")
            if check and existing is None:
                raise ValueError(f"Canonical manifest requires SHA-256: {url}")
            if not check and (refresh or existing is None):
                item["sha256"], size = sha256(url)
                print(f"recorded {url}: {item['sha256']} ({size} B)")
            output.append(item)
        doc[group] = output
    if check:
        print(f"valid: {SRC}")
        return 0
    rendered = "# Canonical installer manifest; maintain with scripts/gen_manifest.py.\n" + yaml.safe_dump(doc, sort_keys=False, allow_unicode=True)
    part = OUT.with_name(OUT.name + ".part")
    try:
        part.write_text(rendered, encoding="utf-8")
        part.replace(OUT)
    finally:
        part.unlink(missing_ok=True)
    print(f"wrote {OUT}")
    return 0


if __name__ == "__main__":
    import argparse
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check", action="store_true", help="Offline validation only; never writes or downloads")
    parser.add_argument("--refresh", action="store_true", help="Re-download even entries with recorded hashes")
    args = parser.parse_args()
    sys.exit(main(refresh=args.refresh, check=args.check))
