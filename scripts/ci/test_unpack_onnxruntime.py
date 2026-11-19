import io
from pathlib import Path
import tempfile
import unittest
import zipfile

from scripts.ci import unpack_onnxruntime


RUNTIMES = ("linux-x64", "linux-aarch64", "win-x64", "win-arm64")
NUGET_URL = "https://www.nuget.org/api/v2/package/Microsoft.ML.OnnxRuntime/1.28.0"


def package(runtime: str, omit: str = "") -> bytes:
    names = ["onnxruntime.dll", "onnxruntime.lib"] if runtime.startswith("win-") else ["libonnxruntime.so"]
    buffer = io.BytesIO()
    with zipfile.ZipFile(buffer, "w") as archive:
        for name in names:
            if name != omit:
                package_runtime = "linux-arm64" if runtime == "linux-aarch64" else runtime
                archive.writestr(f"runtimes/{package_runtime}/native/{name}", name.encode())
        archive.writestr("runtimes/other/native/ignored", b"ignored")
        archive.writestr("README.md", b"ignored")
    return buffer.getvalue()


class UnpackTests(unittest.TestCase):
    def test_extracts_only_the_requested_runtime(self):
        for runtime in RUNTIMES:
            with self.subTest(runtime=runtime), tempfile.TemporaryDirectory() as directory:
                root = Path(directory)
                source = root / "onnxruntime.nupkg"
                source.write_bytes(package(runtime))
                output = root / "output"
                unpack_onnxruntime.unpack(source, runtime, output)
                expected = {"onnxruntime.dll", "onnxruntime.lib"} if runtime.startswith("win-") else {"libonnxruntime.so"}
                self.assertEqual({path.name for path in (output / "lib").iterdir()}, expected)

    def test_rejects_an_incomplete_runtime(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            source = root / "onnxruntime.nupkg"
            source.write_bytes(package("win-x64", omit="onnxruntime.dll"))
            with self.assertRaisesRegex(ValueError, "onnxruntime.dll"):
                unpack_onnxruntime.unpack(source, "win-x64", root / "output")

    def test_rejects_an_unknown_runtime(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            source = root / "onnxruntime.nupkg"
            source.write_bytes(package("linux-x64"))
            with self.assertRaisesRegex(ValueError, "unsupported runtime"):
                unpack_onnxruntime.unpack(source, "win32", root / "output")

    def test_lock_pins_the_official_package_for_every_desktop_runtime(self):
        lock = Path(__file__).with_name("deps.lock").read_text(encoding="utf-8")
        rows = {}
        for line in lock.splitlines():
            fields = line.split()
            if fields and not line.startswith("#"):
                rows[fields[0]] = (fields[1], fields[2])
        digests = set()
        for runtime in ("linux-x64", "linux-aarch64", "win-x64", "win-arm64"):
            url, digest = rows[f"onnxruntime-{runtime}"]
            self.assertEqual(url, NUGET_URL)
            self.assertEqual(len(digest), 64)
            int(digest, 16)
            digests.add(digest)
        self.assertEqual(len(digests), 1)


if __name__ == "__main__":
    unittest.main()
