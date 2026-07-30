#!/usr/bin/env python3
"""Build, boot, validate, and capture the Mini Determinator QEMU demo."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
import selectors
import shutil
import socket
import subprocess
import sys
import tempfile
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
DEMO = ROOT / "demos" / "mini-determinator-qemu"
DEFAULT_OUTPUT = ROOT / "docs" / "assets" / "marketing"
TOOLCHAIN = "nightly-2026-07-21"
START_MARKER = "ZENOFCIS_QEMU_DEMO/1"
END_MARKER = "QEMU_DEMO_COMPLETE"
FRAMEBUFFER_PATTERN = re.compile(r"FRAMEBUFFER=([1-9][0-9]*)x([1-9][0-9]*)")

EXPECTED_PREFIX = (
    START_MARKER,
    "BOOT=KERNEL",
    "FIRMWARE_HANDOFF=COMPLETE",
    "TARGET=x86_64-unknown-none",
    "CORE=zeno-fcis-spec/1.0.0-rc.3",
    "REPLAY_ORDER_A=2,1",
    "REPLAY_ORDER_B=1,2",
    "REPLAY=PASS",
    "SLOT_1=10",
    "SLOT_2=15",
    "SLOT_3=20",
    "WORKER_1_RETURN=15",
    "WORKER_2_RETURN=20",
    "CONFLICT=SLOT_4:WORKERS_1_2",
    "AUTHORITY_CHANGE=NONE",
)


class DemoError(RuntimeError):
    """A fail-closed demo build, execution, or validation error."""


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def executable(name: str) -> Path:
    resolved = shutil.which(name)
    if resolved is None:
        raise DemoError(f"required executable is unavailable: {name}")
    # Preserve multicall/symlink names such as `cargo -> rustup`; argv[0]
    # selects the intended tool behavior.
    return Path(resolved)


def qemu_runtime() -> tuple[Path, Path, Path, dict[str, str], list[str]]:
    configured = os.environ.get("ZENO_QEMU_ROOT")
    local_root = Path(configured).resolve() if configured else None
    if local_root is None:
        system_qemu = shutil.which("qemu-system-x86_64")
        if system_qemu is not None:
            code, variables = find_ovmf(None)
            return Path(system_qemu).resolve(), code, variables, os.environ.copy(), []
        fallback = Path("/tmp/zeno-qemu-local")
        if (fallback / "usr/bin/qemu-system-x86_64").is_file():
            local_root = fallback

    if local_root is None:
        raise DemoError(
            "qemu-system-x86_64 is unavailable; install QEMU and OVMF or set "
            "ZENO_QEMU_ROOT to an extracted package tree"
        )

    qemu = local_root / "usr/bin/qemu-system-x86_64"
    if not qemu.is_file():
        raise DemoError(f"QEMU binary is absent under ZENO_QEMU_ROOT: {qemu}")
    code, variables = find_ovmf(local_root)
    environment = os.environ.copy()
    library_dir = local_root / "usr/lib/x86_64-linux-gnu"
    module_dir = library_dir / "qemu"
    environment["QEMU_MODULE_DIR"] = str(module_dir)
    previous = environment.get("LD_LIBRARY_PATH")
    environment["LD_LIBRARY_PATH"] = (
        f"{library_dir}:{previous}" if previous else str(library_dir)
    )
    data_dir = local_root / "usr/share/qemu"
    extra = ["-L", str(data_dir)]
    return qemu.resolve(), code, variables, environment, extra


def find_ovmf(local_root: Path | None) -> tuple[Path, Path]:
    prefix = local_root / "usr" if local_root is not None else Path("/usr")
    pairs = (
        (
            prefix / "share/OVMF/OVMF_CODE_4M.fd",
            prefix / "share/OVMF/OVMF_VARS_4M.fd",
        ),
        (
            prefix / "share/OVMF/OVMF_CODE.fd",
            prefix / "share/OVMF/OVMF_VARS.fd",
        ),
        (
            prefix / "share/edk2/x64/OVMF_CODE.fd",
            prefix / "share/edk2/x64/OVMF_VARS.fd",
        ),
    )
    for code, variables in pairs:
        if code.is_file() and variables.is_file():
            return code.resolve(), variables.resolve()
    where = local_root if local_root is not None else prefix
    raise DemoError(f"a matching OVMF CODE/VARS firmware pair is absent under {where}")


def build_image() -> Path:
    cargo = executable("cargo")
    command = [
        str(cargo),
        f"+{TOOLCHAIN}",
        "-Z",
        "bindeps",
        "run",
        "--manifest-path",
        str(DEMO / "Cargo.toml"),
        "--release",
        "--locked",
        "--quiet",
    ]
    completed = subprocess.run(
        command,
        cwd=ROOT,
        check=False,
        capture_output=True,
        text=True,
        shell=False,
    )
    if completed.returncode != 0:
        raise DemoError(
            "kernel image build failed\n"
            f"stdout:\n{completed.stdout}\n"
            f"stderr:\n{completed.stderr}"
        )
    candidates = [Path(line) for line in completed.stdout.splitlines() if line.endswith(".img")]
    if len(candidates) != 1 or not candidates[0].is_file():
        raise DemoError("kernel build did not report exactly one existing UEFI image")
    return candidates[0].resolve()


def extract_transcript(raw: bytes) -> tuple[str, tuple[int, int]]:
    text = raw.decode("utf-8", errors="replace").replace("\r", "")
    lines = text.splitlines()
    starts = [index for index, line in enumerate(lines) if line == START_MARKER]
    if len(starts) != 1:
        raise DemoError(f"expected one {START_MARKER!r} marker, found {len(starts)}")
    start = starts[0]
    ends = [index for index, line in enumerate(lines[start:], start) if line == END_MARKER]
    if len(ends) != 1:
        raise DemoError(f"expected one {END_MARKER!r} marker after start, found {len(ends)}")
    block = lines[start : ends[0] + 1]
    if any("KERNEL_PANIC=" in line for line in block):
        raise DemoError("guest reported a kernel panic")
    expected_length = len(EXPECTED_PREFIX) + 2
    if len(block) != expected_length:
        raise DemoError(
            f"guest transcript has {len(block)} fields; expected {expected_length}: {block!r}"
        )
    if tuple(block[: len(EXPECTED_PREFIX)]) != EXPECTED_PREFIX:
        raise DemoError("guest semantic transcript differs from the closed expected result")
    framebuffer = FRAMEBUFFER_PATTERN.fullmatch(block[-2])
    if framebuffer is None:
        raise DemoError(f"invalid framebuffer result: {block[-2]!r}")
    dimensions = (int(framebuffer.group(1)), int(framebuffer.group(2)))
    if dimensions[0] < 800 or dimensions[1] < 600:
        raise DemoError(f"framebuffer is too small for the bounded demo UI: {dimensions}")
    if block[-1] != END_MARKER:
        raise DemoError("guest completion marker is not final")
    return "\n".join(block) + "\n", dimensions


def fixed_qemu_argv(
    qemu: Path,
    firmware_code: Path,
    firmware_variables: Path,
    disk: Path,
    monitor: Path,
    extra: list[str],
) -> list[str]:
    return [
        str(qemu),
        *extra,
        "-machine",
        "q35",
        "-accel",
        "tcg,thread=single",
        "-cpu",
        "max",
        "-smp",
        "1",
        "-m",
        "128M",
        "-nic",
        "none",
        "-drive",
        f"if=pflash,format=raw,unit=0,file={firmware_code},readonly=on",
        "-drive",
        f"if=pflash,format=raw,unit=1,file={firmware_variables}",
        "-drive",
        f"format=raw,file={disk}",
        "-serial",
        "stdio",
        "-display",
        "none",
        "-monitor",
        f"unix:{monitor},server=on,wait=off",
        "-no-reboot",
        "-no-shutdown",
    ]


def monitor_command(monitor: Path, command: str, deadline: float) -> str:
    last_error: OSError | None = None
    while time.monotonic() < deadline:
        client = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        try:
            client.connect(str(monitor))
            receive_monitor_prompt(client, deadline)
            client.sendall(command.encode("utf-8") + b"\n")
            response = receive_monitor_prompt(client, deadline)
            client.close()
            return response
        except OSError as error:
            last_error = error
            client.close()
            time.sleep(0.05)
    raise DemoError(f"QEMU monitor did not accept {command!r}: {last_error}")


def receive_monitor_prompt(client: socket.socket, deadline: float) -> str:
    response = bytearray()
    while time.monotonic() < deadline:
        client.settimeout(min(0.2, max(deadline - time.monotonic(), 0.01)))
        try:
            chunk = client.recv(4096)
        except TimeoutError:
            continue
        if not chunk:
            break
        response.extend(chunk)
        if b"(qemu) " in response:
            return response.decode("utf-8", errors="replace")
    raise DemoError(
        "QEMU monitor prompt timed out: "
        + response.decode("utf-8", errors="replace")
    )


def boot_and_capture(image: Path, timeout_seconds: float) -> tuple[str, Path, dict[str, str]]:
    qemu, firmware_code, firmware_template, environment, extra = qemu_runtime()
    with tempfile.TemporaryDirectory(prefix="zeno-qemu-demo-") as temporary:
        temp = Path(temporary)
        firmware_variables = temp / "OVMF_VARS.fd"
        disk = temp / "demo.img"
        monitor = temp / "monitor.sock"
        framebuffer = temp / "framebuffer.ppm"
        shutil.copyfile(firmware_template, firmware_variables)
        shutil.copyfile(image, disk)
        command = fixed_qemu_argv(
            qemu,
            firmware_code,
            firmware_variables,
            disk,
            monitor,
            extra,
        )
        process = subprocess.Popen(
            command,
            cwd=ROOT,
            env=environment,
            stdin=subprocess.DEVNULL,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            shell=False,
        )
        if process.stdout is None:
            process.kill()
            raise DemoError("QEMU serial stream was not created")
        selector = selectors.DefaultSelector()
        selector.register(process.stdout, selectors.EVENT_READ)
        deadline = time.monotonic() + timeout_seconds
        raw = bytearray()
        try:
            while time.monotonic() < deadline and END_MARKER.encode() not in raw:
                if process.poll() is not None:
                    remainder = process.stdout.read()
                    if remainder:
                        raw.extend(remainder)
                    break
                for key, _ in selector.select(timeout=0.1):
                    chunk = os.read(key.fileobj.fileno(), 65536)
                    if chunk:
                        raw.extend(chunk)
            transcript, dimensions = extract_transcript(bytes(raw))
            monitor_response = monitor_command(
                monitor, f"screendump {framebuffer}", time.monotonic() + 5.0
            )
            capture_deadline = time.monotonic() + 5.0
            while time.monotonic() < capture_deadline and not framebuffer.is_file():
                time.sleep(0.05)
            if not framebuffer.is_file() or framebuffer.stat().st_size == 0:
                raise DemoError(
                    "QEMU did not produce the requested framebuffer dump; "
                    f"monitor response: {monitor_response!r}"
                )
            descriptor, captured_name = tempfile.mkstemp(
                prefix="zeno-framebuffer-", suffix=".ppm"
            )
            os.close(descriptor)
            captured = Path(captured_name)
            shutil.copyfile(framebuffer, captured)
            metadata = {
                "disk_image_sha256": sha256(image),
                "framebuffer": f"{dimensions[0]}x{dimensions[1]}",
                "qemu": subprocess.run(
                    [str(qemu), "--version"],
                    env=environment,
                    check=False,
                    capture_output=True,
                    text=True,
                    shell=False,
                ).stdout.splitlines()[0],
                "rust_toolchain": TOOLCHAIN,
                "schema": "zeno-fcis-qemu-demo-capture/1",
            }
            return transcript, captured, metadata
        finally:
            selector.close()
            if process.poll() is None:
                try:
                    monitor_command(monitor, "quit", time.monotonic() + 2.0)
                    process.wait(timeout=2.0)
                except (DemoError, subprocess.TimeoutExpired):
                    process.kill()
                    process.wait(timeout=2.0)


def convert_capture(ppm: Path, png: Path) -> None:
    converter = executable("convert")
    png.parent.mkdir(parents=True, exist_ok=True)
    completed = subprocess.run(
        [str(converter), str(ppm), "-strip", str(png)],
        cwd=ROOT,
        check=False,
        capture_output=True,
        text=True,
        shell=False,
    )
    if completed.returncode != 0 or not png.is_file():
        raise DemoError(f"framebuffer conversion failed: {completed.stderr}")


def run_demo(capture: bool, output: Path, timeout_seconds: float) -> None:
    image = build_image()
    transcript, ppm, metadata = boot_and_capture(image, timeout_seconds)
    try:
        if capture:
            png = output / "mini-determinator-qemu-kernel.png"
            transcript_path = output / "mini-determinator-qemu-serial.txt"
            metadata_path = output / "mini-determinator-qemu-capture.json"
            convert_capture(ppm, png)
            transcript_path.write_text(transcript, encoding="utf-8")
            metadata["framebuffer_png_sha256"] = sha256(png)
            metadata["guest_transcript_sha256"] = hashlib.sha256(
                transcript.encode("utf-8")
            ).hexdigest()
            metadata_path.write_text(
                json.dumps(metadata, indent=2, sort_keys=True) + "\n",
                encoding="utf-8",
            )
            print(f"capture: {png}")
            print(f"serial: {transcript_path}")
            print(f"metadata: {metadata_path}")
        print(transcript, end="")
    finally:
        ppm.unlink(missing_ok=True)


def doctor() -> None:
    checks: list[tuple[str, str]] = []
    checks.append(("cargo", str(executable("cargo"))))
    checks.append(("convert", str(executable("convert"))))
    qemu, code, variables, environment, _ = qemu_runtime()
    version = subprocess.run(
        [str(qemu), "--version"],
        env=environment,
        check=False,
        capture_output=True,
        text=True,
        shell=False,
    )
    if version.returncode != 0:
        raise DemoError(f"QEMU version check failed: {version.stderr}")
    checks.extend(
        (
            ("qemu", version.stdout.splitlines()[0]),
            ("ovmf-code", str(code)),
            ("ovmf-vars", str(variables)),
            ("toolchain", TOOLCHAIN),
        )
    )
    for name, value in checks:
        print(f"{name}: {value}")


def self_test() -> None:
    valid = "\n".join((*EXPECTED_PREFIX, "FRAMEBUFFER=1280x800", END_MARKER)) + "\n"
    transcript, dimensions = extract_transcript(valid.encode())
    assert transcript == valid
    assert dimensions == (1280, 800)
    hostile = (
        valid.replace("REPLAY=PASS", "REPLAY=FAIL"),
        valid.replace("AUTHORITY_CHANGE=NONE", "AUTHORITY_CHANGE=UNSAFE"),
        valid.replace(END_MARKER, "KERNEL_PANIC=boom\n" + END_MARKER),
        valid.replace("SLOT_2=15\n", "SLOT_2=15\nSLOT_2=15\n"),
        valid.replace("FRAMEBUFFER=1280x800", "FRAMEBUFFER=320x200"),
        valid.replace(END_MARKER, ""),
    )
    for case in hostile:
        try:
            extract_transcript(case.encode())
        except DemoError:
            continue
        raise AssertionError(f"hostile transcript was accepted: {case!r}")
    argv = fixed_qemu_argv(
        Path("/qemu"),
        Path("/code"),
        Path("/vars"),
        Path("/disk"),
        Path("/monitor"),
        [],
    )
    assert argv[0] == "/qemu"
    assert "-nic" in argv and argv[argv.index("-nic") + 1] == "none"
    assert "-smp" in argv and argv[argv.index("-smp") + 1] == "1"
    assert not {"sh", "bash", "dash", "zsh"}.intersection(argv)
    print("qemu-demo-self-test: PASS")


def parser() -> argparse.ArgumentParser:
    result = argparse.ArgumentParser(description=__doc__)
    subcommands = result.add_subparsers(dest="command", required=True)
    subcommands.add_parser("doctor", help="check host demo prerequisites")
    subcommands.add_parser("build", help="build and print the UEFI disk image path")
    run = subcommands.add_parser("run", help="boot QEMU and validate the guest transcript")
    run.add_argument("--timeout", type=float, default=30.0)
    capture = subcommands.add_parser(
        "capture", help="boot, validate, and save the real framebuffer and serial evidence"
    )
    capture.add_argument("--timeout", type=float, default=30.0)
    capture.add_argument("--out", type=Path, default=DEFAULT_OUTPUT)
    subcommands.add_parser("self-test", help="exercise fail-closed transcript validation")
    return result


def main() -> int:
    arguments = parser().parse_args()
    try:
        if arguments.command == "doctor":
            doctor()
        elif arguments.command == "build":
            print(build_image())
        elif arguments.command == "run":
            run_demo(False, DEFAULT_OUTPUT, arguments.timeout)
        elif arguments.command == "capture":
            run_demo(True, arguments.out.resolve(), arguments.timeout)
        elif arguments.command == "self-test":
            self_test()
        else:
            raise AssertionError(f"unhandled command: {arguments.command}")
        return 0
    except DemoError as error:
        print(f"qemu-demo: FAIL: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
