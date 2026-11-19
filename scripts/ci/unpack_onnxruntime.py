"""Unpack one native runtime from Microsoft's ONNX Runtime NuGet package."""

import argparse
from pathlib import Path, PurePosixPath
import zipfile


REQUIRED = {
    "linux-x64": ("libonnxruntime.so",),
    "linux-aarch64": ("libonnxruntime.so",),
    "win-x64": ("onnxruntime.lib", "onnxruntime.dll"),
    "win-arm64": ("onnxruntime.lib", "onnxruntime.dll"),
}


def unpack(source: Path, runtime: str, output: Path) -> None:
    """Put the selected runtime's native libraries in output/lib."""
    if runtime not in REQUIRED:
        raise ValueError(f"unsupported runtime: {runtime}")
    package_runtime = "linux-arm64" if runtime == "linux-aarch64" else runtime
    prefix = f"runtimes/{package_runtime}/native/"
    with zipfile.ZipFile(source) as archive:
        members = [name for name in archive.namelist() if name.startswith(prefix) and not name.endswith("/")]
        names = {PurePosixPath(name).name for name in members}
        missing = set(REQUIRED[runtime]) - names
        if missing:
            raise ValueError(f"ONNX Runtime package is missing: {', '.join(sorted(missing))}")
        library = output / "lib"
        library.mkdir(parents=True, exist_ok=True)
        for member in members:
            destination = library / PurePosixPath(member).name
            destination.write_bytes(archive.read(member))
            if runtime.startswith("linux-"):
                destination.chmod(0o755)
                if destination.name == "libonnxruntime.so":
                    # The NuGet file's SONAME is libonnxruntime.so.1. A hard
                    # link satisfies the loader without Windows symlink rights.
                    (library / "libonnxruntime.so.1").hardlink_to(destination)


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("source", type=Path)
    parser.add_argument("runtime", choices=sorted(REQUIRED))
    parser.add_argument("output", type=Path)
    arguments = parser.parse_args()
    unpack(arguments.source, arguments.runtime, arguments.output)


if __name__ == "__main__":
    main()
