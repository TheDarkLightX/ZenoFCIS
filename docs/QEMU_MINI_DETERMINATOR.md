# Mini Determinator QEMU kernel demo

ZenoFCIS `1.0.0-rc.3` includes an isolated, executable `no_std` x86_64 kernel
demo under `demos/mini-determinator-qemu/`. It boots through OVMF in QEMU,
calls the public `zeno-fcis-spec` Mini Determinator implementation, writes a
host-validated result to COM1, renders a guest framebuffer, and halts.

![Mini Determinator kernel running in QEMU](assets/marketing/mini-determinator-qemu-kernel.png)

This PNG is a direct QEMU framebuffer dump. The associated
[guest serial transcript](assets/marketing/mini-determinator-qemu-serial.txt)
and [capture metadata](assets/marketing/mini-determinator-qemu-capture.json)
were produced by the same validated boot.

## What the guest executes

After firmware handoff, the kernel:

1. initializes a bounded static allocator and COM1 serial output;
2. creates one immutable spawn snapshot with slot 1 equal to 10;
3. executes worker 2 then worker 1, and worker 1 then worker 2;
4. asserts that both complete `MiniRun` values are equal;
5. checks the canonical merged slots `1=10`, `2=15`, and `3=20`;
6. submits two private writes to slot 4;
7. checks the stable conflict witness for workers 1 and 2 and verifies that the
   caller's pre-state remains unchanged; and
8. renders the result only after those checks pass.

The semantic calls use the same `zeno-fcis-spec` crate as the host example and
tests. The display is not a prerecorded boot animation. The text in the visual
is deliberately concise; the serial transcript carries the exact result that
the host validator checks.

## Reproduce the capture

On Ubuntu, install the host-only emulator and image conversion prerequisites:

```bash
sudo apt-get install qemu-system-x86 ovmf imagemagick
rustup toolchain install nightly-2026-07-21 \
  --profile minimal \
  --component llvm-tools-preview \
  --target x86_64-unknown-none
```

Then run from the repository root:

```bash
python3 tools/qemu_demo.py doctor
python3 tools/qemu_demo.py self-test
python3 tools/qemu_demo.py run
python3 tools/qemu_demo.py capture
```

`build` creates a UEFI disk image without starting QEMU. `run` boots and
validates without changing checked-in marketing artifacts. `capture` replaces
the PNG, extracted guest transcript, and content-hash metadata only after a
successful validated boot.

The runner invokes Cargo, QEMU, and ImageMagick with argument arrays and
`shell=False`. QEMU uses single-threaded TCG, one virtual CPU, 128 MiB of RAM,
and no virtual network. The source disk image and OVMF variable template remain
read-only inputs; writable copies live in a temporary directory for each run.
The guest transcript validator rejects missing or duplicate markers, panics,
changed results, changed conflict or authority fields, small or absent
framebuffers, and incomplete execution.

For an extracted QEMU installation, set `ZENO_QEMU_ROOT` to the package-tree
root. The runner also recognizes `/tmp/zeno-qemu-local` as a development
fallback when system QEMU is unavailable.

## Demonstration boundary

This is a real kernel boot and an executable integration demonstration. It is
not a production operating system. The bootloader supplies UEFI loading, page
tables, and framebuffer mapping. The demo kernel supplies a bounded allocator,
serial output, framebuffer rendering, the Mini Determinator call, assertions,
panic reporting, and halt loop.

It has no processes, privilege separation, preemptive scheduler, threads,
interrupt handling, filesystem, network stack, general device drivers, real
inter-process communication, persistent recovery, hardware qualification, or
production authority. QEMU execution demonstrates that the semantic model can
run inside a freestanding kernel environment. It does not prove hardware-wide
determinism or strengthen the scope of the pure merge semantics.

An accurate public caption is:

> ZenoFCIS Mini Determinator running inside a freestanding Rust kernel in QEMU:
> opposite worker completion orders converge, while conflicting private writes
> reject without authoritative state change.
