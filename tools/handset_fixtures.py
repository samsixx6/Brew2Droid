"""Empacota cópias privadas verificadas; nunca grava na coleção original."""
import argparse
import hashlib
import io
import json
from pathlib import Path
import tarfile


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--source", type=Path, required=True)
    parser.add_argument("--destination", type=Path, required=True)
    args = parser.parse_args()
    root = Path(__file__).resolve().parent.parent
    source = args.source.resolve(strict=True)
    destination = args.destination.resolve()
    if not destination.is_relative_to(root / "local-tests") or destination.is_relative_to(source):
        raise ValueError("Fixture output must be private local-tests storage")
    destination.mkdir(parents=True, exist_ok=True)
    catalog = json.loads((root / "tests/scenarios/catalog.json").read_text())
    manifest = {}
    with tarfile.open(destination / "handset-games.tar", "w") as archive:
        for game in catalog["games"]:
            module = (source / game["module"]).resolve(strict=True)
            mif = (source / game["mif"]).resolve(strict=True)
            if not module.is_relative_to(source) or not mif.is_relative_to(source):
                raise ValueError("Fixture escaped the source collection")
            if hashlib.sha256(module.read_bytes()).hexdigest() != game["module_sha256"]:
                raise ValueError("Module differs from the approved catalog")
            entries = [(p, Path(game["id"]) / p.relative_to(module.parent))
                       for p in sorted(module.parent.rglob("*")) if p.is_file()]
            entries.append((mif, Path(game["id"]) / module.with_suffix(".mif").name))
            for path, relative in entries:
                if path.is_symlink() or not path.resolve().is_relative_to(source):
                    raise ValueError("Fixture resource escaped its source")
                data = path.read_bytes()
                name = relative.as_posix()
                info = tarfile.TarInfo(name)
                info.size, info.mode = len(data), 0o600
                archive.addfile(info, io.BytesIO(data))
                manifest[name] = hashlib.sha256(data).hexdigest()
    (destination / "manifest.json").write_text(json.dumps(manifest, indent=2) + "\n")
    print(f"Prepared {len(manifest)} private fixture files")


if __name__ == "__main__":
    main()
