"""Exercise a built installer without touching a real Minecraft directory."""
import pathlib
import subprocess
import sys
import tempfile


def main():
    binary = pathlib.Path(sys.argv[1]).resolve(strict=True)
    with tempfile.TemporaryDirectory(prefix="currspice release smoke ") as temp:
        root = pathlib.Path(temp)
        mc = root / "Minecraft with spaces"
        mc.mkdir()
        (mc / "keep.txt").write_bytes(b"must survive unchanged")

        def snapshot():
            return {str(p.relative_to(root)): p.read_bytes() if p.is_file() else None
                    for p in root.rglob("*")}

        def run(args, expected, needle):
            before = snapshot()
            result = subprocess.run([str(binary), *args], cwd=root,
                                    capture_output=True, text=True, timeout=30)
            output = result.stdout + result.stderr
            if result.returncode != expected or needle.lower() not in output.lower():
                raise RuntimeError(f"{args}: exit {result.returncode}\n{output}")
            if snapshot() != before:
                raise RuntimeError(f"{args}: modified the temporary fixture")
            print(f"PASS: {' '.join(args)} (exit {expected}; fixture unchanged)")

        run(["--help"], 0, "--dry-run")
        run(["--dry-run", "--verbose", "--mc-dir", str(mc)], 0, "manifest OK")
        run(["--dry-run", "--config", str(root / "missing.yaml"),
             "--mc-dir", str(mc)], 1, "manifest")
    print("Release smoke passed: help, embedded manifest dry-run, missing config; no fixture changes.")


if __name__ == "__main__":
    main()
