import importlib.util
import copy
from pathlib import Path
import struct
import tempfile
import unittest
import zipfile

SPEC = importlib.util.spec_from_file_location("apk_checks", Path(__file__).parents[1] / "apk_checks.py")
checks = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(checks)

BADGING = """package: name='io.github.brew.handset' compileSdkVersion='37'
minSdkVersion:'26'
targetSdkVersion:'37'
native-code: 'arm64-v8a'
"""
CAPABILITIES = {
    "schema_version": 1, "host_arch": "aarch64", "cpu_backend": "dynarmic",
    "native_page_size_bytes": 4096,
    "cpu_probe": {"arm_result": 42, "thumb_result": 8, "unmapped_fault_addr": 0x3000, "instructions": 6,
                  "memory_protection_passed": True},
    "graphics": {"vendor": "vendor", "renderer": "GPU", "version": "OpenGL ES 3.2",
                 "triangle_readback_rgba": [64, 128, 192, 255], "triangle_probe_passed": True},
}


def elf(alignment=16384, machine=183):
    data = bytearray(120)
    data[:7] = b"\x7fELF\x02\x01\x01"
    struct.pack_into("<HHIQQQIHHHHHH", data, 16, 3, machine, 1, 0, 64, 0, 0, 64, 56, 1, 0, 0, 0)
    struct.pack_into("<IIQQQQQQ", data, 64, 1, 5, 0, 0, 0, 120, 120, alignment)
    return bytes(data)


class ApkChecks(unittest.TestCase):
    def inspect(self, libs=None, badging=BADGING):
        root = Path(__file__).resolve().parents[2]
        scratch = root / ".superpowers"
        scratch.mkdir(exist_ok=True)
        with tempfile.TemporaryDirectory(dir=scratch) as directory:
            assert Path(directory).resolve().is_relative_to(root)
            path = Path(directory) / "test.apk"
            with zipfile.ZipFile(path, "w", compression=zipfile.ZIP_STORED) as archive:
                for name, data in (libs or {"lib/arm64-v8a/libzeebx_android.so": elf()}).items():
                    archive.writestr(name, data)
            return checks.inspect_apk(path, badging)

    def test_accepts_arm64_elf_with_16k_load_alignment(self):
        result = self.inspect()
        self.assertEqual(result["native_libraries"], ["lib/arm64-v8a/libzeebx_android.so"])
        self.assertEqual(result["target_sdk"], 37)

    def test_rejects_4k_library_in_modern_apk(self):
        with self.assertRaisesRegex(ValueError, "16 KB"):
            self.inspect({"lib/arm64-v8a/libzeebx_android.so": elf(4096)})

    def test_rejects_wrong_elf_machine_even_in_arm64_directory(self):
        with self.assertRaisesRegex(ValueError, "AArch64"):
            self.inspect({"lib/arm64-v8a/libzeebx_android.so": elf(machine=62)})

    def test_rejects_extra_32bit_abi(self):
        with self.assertRaisesRegex(ValueError, "ABI"):
            self.inspect({"lib/arm64-v8a/libzeebx_android.so": elf(), "lib/armeabi-v7a/libother.so": elf()})

    def test_rejects_old_target_or_compile_sdk(self):
        for old in (BADGING.replace("targetSdkVersion:'37'", "targetSdkVersion:'35'"),
                    BADGING.replace("compileSdkVersion='37'", "compileSdkVersion='35'")):
            with self.subTest(badging=old), self.assertRaisesRegex(ValueError, "SDK"):
                self.inspect(badging=old)

    def test_rejects_truncated_program_headers(self):
        with self.assertRaisesRegex(ValueError, "ELF"):
            self.inspect({"lib/arm64-v8a/libzeebx_android.so": elf()[:100]})

    def test_rejects_missing_native_library(self):
        with self.assertRaisesRegex(ValueError, "native"):
            self.inspect({"assets/test.txt": b"test"})

    def test_rejects_missing_cpu_or_gpu_proof(self):
        with self.assertRaisesRegex(ValueError, "capability"):
            checks.check_capabilities({"schema_version": 1})

    def test_4k_runtime_does_not_prove_16k_jit(self):
        result = checks.check_capabilities(CAPABILITIES)
        self.assertTrue(result["cpu_probe_passed"])
        self.assertTrue(result["gpu_probe_passed"])
        self.assertFalse(result["runtime_16kb_verified"])

    def test_wrong_triangle_pixel_is_rejected_even_with_success_flag(self):
        report = copy.deepcopy(CAPABILITIES)
        report["graphics"]["triangle_readback_rgba"] = [0, 0, 0, 255]
        with self.assertRaisesRegex(ValueError, "capability"):
            checks.check_capabilities(report)

    def test_wrong_cpu_result_is_rejected(self):
        report = copy.deepcopy(CAPABILITIES)
        report["cpu_probe"]["thumb_result"] = 7
        with self.assertRaisesRegex(ValueError, "capability"):
            checks.check_capabilities(report)

    def test_missing_or_failed_memory_protection_proof_is_rejected(self):
        for value in (None, False):
            report = copy.deepcopy(CAPABILITIES)
            if value is None:
                del report["cpu_probe"]["memory_protection_passed"]
            else:
                report["cpu_probe"]["memory_protection_passed"] = value
            with self.subTest(value=value), self.assertRaisesRegex(ValueError, "capability"):
                checks.check_capabilities(report)

    def test_runtime_proof_requires_the_same_apk_hash(self):
        capture = {"schema_version": 1, "apk_sha256": "a" * 64, "native_report": CAPABILITIES}
        with self.assertRaisesRegex(ValueError, "APK hash"):
            checks.check_bound_capabilities(CAPABILITIES, capture, "b" * 64)

    def test_runtime_proof_rejects_a_different_native_report(self):
        capture = {"schema_version": 1, "apk_sha256": "a" * 64, "native_report": {}}
        with self.assertRaisesRegex(ValueError, "capture record"):
            checks.check_bound_capabilities(CAPABILITIES, capture, "a" * 64)

    def test_accepts_runtime_proof_bound_to_this_apk(self):
        capture = {"schema_version": 1, "apk_sha256": "a" * 64, "native_report": CAPABILITIES}
        result = checks.check_bound_capabilities(CAPABILITIES, capture, "a" * 64)
        self.assertTrue(result["cpu_probe_passed"])


if __name__ == "__main__":
    unittest.main()
