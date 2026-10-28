"""Registra os pacotes sem gravar na coleção original."""

import argparse
from datetime import datetime, timezone
import hashlib
import json
from pathlib import Path
import struct
import sys
import tempfile


DEFAULT_CATALOG = Path(__file__).resolve().parents[1] / "tests" / "scenarios" / "catalog.json"


def source_file(root, relative):
    path = (root / relative).resolve()
    if not path.is_relative_to(root):
        raise ValueError(f"path outside the source collection: {relative}")
    if not path.is_file():
        raise ValueError(f"missing file: {relative}")
    return path


def build_manifest(root, catalog):
    if catalog.get("schema_version") != 1 or not catalog.get("games"):
        raise ValueError("catalog must contain schema_version 1 and games")
    games = []
    for entry in catalog["games"]:
        module = source_file(root, entry["module"])
        mif = source_file(root, entry["mif"])
        paths = {mif, module}
        for path in module.parent.rglob("*"):
            if path.is_file():
                paths.add(source_file(root, path.relative_to(root)))
        files = []
        for path in sorted(paths):
            data = path.read_bytes()
            digest = hashlib.sha256(data).hexdigest()
            if path == module and digest != entry["module_sha256"].lower():
                raise ValueError(f"module hash mismatch: {entry['id']}")
            role = "resource"
            if path == module:
                role = "module"
            elif path == mif:
                role = "mif"
            elif path.name == "jamdatbowling3dsave.dat2":
                role = "existing_save"
            files.append({"path": path.relative_to(root).as_posix(),
                          "size_bytes": len(data), "sha256": digest, "role": role})
        mif_data = mif.read_bytes()
        games.append({"id": entry["id"], "module": entry["module"], "mif": entry["mif"],
                      "mif_first_words_le": list(struct.unpack_from("<HH", mif_data)) if len(mif_data) >= 4 else None,
                      "file_count": len(files), "total_size_bytes": sum(f["size_bytes"] for f in files),
                      "files": files})
    return {"schema_version": 1, "inventory_utc": datetime.now(timezone.utc).isoformat(),
            "collection_root": root.name, "games": games}


def main():
    parser = argparse.ArgumentParser(description="Inventory verified BREW packages without changing the source collection.")
    parser.add_argument("--root", type=Path, required=True)
    parser.add_argument("--catalog", type=Path, default=DEFAULT_CATALOG)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    temporary = None
    try:
        root = args.root.resolve(strict=True)
        output = args.output.resolve()
        if output.is_relative_to(root):
            raise ValueError("output must be outside the source collection")
        catalog = json.loads(args.catalog.read_text(encoding="utf-8-sig"))
        report = build_manifest(root, catalog)
        output.parent.mkdir(parents=True, exist_ok=True)
        # Só publica um relatório inteiro, após conferir todos os hashes.
        with tempfile.NamedTemporaryFile(mode="w", encoding="utf-8", dir=output.parent, delete=False) as stream:
            temporary = Path(stream.name)
            stream.write(json.dumps(report, indent=2) + "\n")
        temporary.replace(output)
        for game in report["games"]:
            print(f"{game['id']}: {game['file_count']} files, {game['total_size_bytes']} bytes")
        return 0
    except (OSError, ValueError, KeyError, TypeError) as error:
        print(f"Inventory failed: {error}", file=sys.stderr)
        return 1
    finally:
        if temporary is not None and temporary.exists():
            temporary.unlink()


if __name__ == "__main__":
    raise SystemExit(main())
