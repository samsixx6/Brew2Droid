"""Confere o APK real; alinhamento do arquivo não comprova execução em página de 16 KB."""

import argparse
import hashlib
import json
from pathlib import Path
import re
import struct
import subprocess
import zipfile


def check_elf(data, name):
    if len(data) < 64 or data[:7] != b"\x7fELF\x02\x01\x01":
        raise ValueError(f"Invalid ELF64 little-endian library: {name}")
    if struct.unpack_from("<H", data, 18)[0] != 183:
        raise ValueError(f"Native library is not AArch64: {name}")
    offset = struct.unpack_from("<Q", data, 32)[0]
    entry_size, count = struct.unpack_from("<HH", data, 54)
    if entry_size < 56 or not count or offset + entry_size * count > len(data):
        raise ValueError(f"Invalid ELF program headers: {name}")
    loads = 0
    for index in range(count):
        kind, _, file_offset, address, _, size, memory_size, alignment = struct.unpack_from(
            "<IIQQQQQQ", data, offset + index * entry_size)
        if kind != 1:
            continue
        loads += 1
        if size > memory_size or file_offset + size > len(data):
            raise ValueError(f"Invalid ELF load range: {name}")
        if (alignment < 16384 or alignment & (alignment - 1)
                or (file_offset - address) % 16384):
            raise ValueError(f"Native library does not support 16 KB load alignment: {name}")
    if not loads:
        raise ValueError(f"ELF has no load segments: {name}")


def inspect_apk(path, badging):
    def sdk(pattern):
        found = re.search(pattern, badging)
        if not found:
            raise ValueError("APK SDK metadata is missing")
        return int(found[1])

    compile_sdk = sdk(r"compileSdkVersion='(\d+)'")
    target_sdk = sdk(r"targetSdkVersion:'(\d+)'")
    min_sdk = sdk(r"minSdkVersion:'(\d+)'")
    if compile_sdk != 37 or target_sdk != 37 or min_sdk != 26:
        raise ValueError("APK SDK contract requires compile/target 37 and minimum 26")
    if "name='io.github.brew.handset'" not in badging:
        raise ValueError("Unexpected Android application ID")
    with zipfile.ZipFile(path) as archive:
        names = archive.namelist()
        if len(names) != len(set(names)):
            raise ValueError("APK contains duplicate entries")
        libraries = sorted(name for name in names if name.startswith("lib/") and name.endswith(".so"))
        if "lib/arm64-v8a/libzeebx_android.so" not in libraries:
            raise ValueError("APK is missing its native emulator library")
        if any(name.split("/")[1] != "arm64-v8a" for name in libraries):
            raise ValueError("APK contains an unexpected ABI")
        for name in libraries:
            if archive.getinfo(name).compress_type != zipfile.ZIP_STORED:
                raise ValueError(f"Native library must be uncompressed: {name}")
            check_elf(archive.read(name), name)
    return {"compile_sdk": compile_sdk, "target_sdk": target_sdk, "min_sdk": min_sdk,
            "abi": "arm64-v8a", "native_libraries": libraries, "elf_16kb_aligned": True}


def check_capabilities(report):
    try:
        cpu, graphics = report["cpu_probe"], report["graphics"]
        rgba = graphics["triangle_readback_rgba"]
        valid = (report["schema_version"] == 1 and report["host_arch"] == "aarch64"
                 and report["cpu_backend"] == "dynarmic"
                 and report["native_page_size_bytes"] in (4096, 16384)
                 and cpu["arm_result"] == 42 and cpu["thumb_result"] == 8
                 and cpu["unmapped_fault_addr"] == 0x3000 and cpu["instructions"] > 0
                 and cpu["memory_protection_passed"] is True
                 and all(graphics[key] for key in ("vendor", "renderer", "version"))
                 and re.search(r"OpenGL ES (3|4)\.", graphics["version"])
                 and graphics["triangle_probe_passed"] is True
                 and len(rgba) == 4
                 and all(abs(value - expected) <= 2 for value, expected in zip(rgba, [64, 128, 192, 255])))
    except (KeyError, TypeError, ValueError):
        valid = False
    if not valid:
        raise ValueError("Native capability report is missing successful ARM/Thumb/memory-protection/fault/GLES proof")
    return {"cpu_probe_passed": True, "gpu_probe_passed": True,
            "runtime_16kb_verified": report["native_page_size_bytes"] == 16384,
            "graphics": graphics}


def check_bound_capabilities(report, capture, apk_sha256):
    if (not isinstance(capture, dict) or capture.get("schema_version") != 1
            or capture.get("native_report") != report):
        raise ValueError("Native capture record is missing or describes a different capability report")
    if str(capture.get("apk_sha256", "")).lower() != apk_sha256:
        raise ValueError("Native runtime proof APK hash does not match the selected APK")
    return check_capabilities(report)


def check_pins(root):
    lock = json.loads((root / "tools/toolchain-lock.json").read_text(encoding="utf-8"))
    android = lock["android"]
    expected = {"agp": "9.4.0", "gradle": "9.6.0", "ndk": "28.2.13676358", "jdk_major": 17}
    if lock["rust"] != "1.99.1" or any(android.get(key) != value for key, value in expected.items()):
        raise ValueError("Toolchain pins do not match the approved build contract")
    rust = (root / "rust-toolchain.toml").read_text(encoding="utf-8")
    gradle = (root / "frontends/android/apk/build.gradle.kts").read_text(encoding="utf-8")
    wrapper = (root / "frontends/android/apk/gradle/wrapper/gradle-wrapper.properties").read_text(encoding="utf-8")
    if ('channel = "1.99.1"' not in rust or 'version "9.4.0"' not in gradle
            or "gradle-9.6.0-bin.zip" not in wrapper
            or f'distributionSha256Sum={android["gradle_distribution_sha256"]}' not in wrapper):
        raise ValueError("Compiler or Gradle dependency pins are missing")
    cargo = (root / "Cargo.lock").read_text(encoding="utf-8")
    if not re.search(r'name = "dynarmic"\s+version = "0\.1\.3"', cargo):
        raise ValueError("Dynarmic dependency pin is missing")
    return lock


def main():
    parser = argparse.ArgumentParser(description="Verify the Android build and optional native device proof.")
    parser.add_argument("--apk", required=True, type=Path)
    parser.add_argument("--aapt2", required=True, type=Path)
    parser.add_argument("--zipalign", required=True, type=Path)
    parser.add_argument("--capabilities", type=Path)
    parser.add_argument("--probe-run", type=Path)
    parser.add_argument("--package-only", action="store_true")
    parser.add_argument("--require-16kb-runtime", action="store_true")
    parser.add_argument("--output", type=Path)
    args = parser.parse_args()
    try:
        pins = check_pins(Path(__file__).resolve().parents[1])
        badging = subprocess.check_output([str(args.aapt2), "dump", "badging", str(args.apk)], text=True)
        result = inspect_apk(args.apk, badging)
        result["apk_sha256"] = hashlib.sha256(args.apk.read_bytes()).hexdigest()
        subprocess.run([str(args.zipalign), "-c", "-P", "16", "4", str(args.apk)], check=True)
        result.update({"schema_version": 1, "zip_16kb_aligned": True, "toolchain": pins})
        if args.capabilities:
            capture_path = args.probe_run or args.capabilities.with_name("probe-run.json")
            report = json.loads(args.capabilities.read_text(encoding="utf-8-sig"))
            capture = json.loads(capture_path.read_text(encoding="utf-8-sig"))
            result.update(check_bound_capabilities(report, capture, result["apk_sha256"]))
        elif not args.package_only:
            raise ValueError("A native capability report is required; use --package-only before installation")
        else:
            result.update({"cpu_probe_passed": None, "gpu_probe_passed": None, "runtime_16kb_verified": False})
        if args.require_16kb_runtime and not result["runtime_16kb_verified"]:
            raise ValueError("Execution on a verified 16 KB device/environment remains untested")
        if args.output:
            args.output.parent.mkdir(parents=True, exist_ok=True)
            args.output.write_text(json.dumps(result, indent=2) + "\n", encoding="utf-8")
        print(json.dumps(result, indent=2))
    except (ValueError, OSError, subprocess.CalledProcessError, zipfile.BadZipFile) as error:
        parser.exit(1, f"APK check failed: {error}\n")


if __name__ == "__main__":
    main()
