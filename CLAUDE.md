# CLAUDE.md

This file provides guidance for interacting with the Zed OS codebase.

## Project Overview

Zed OS is an operating system built with the Zed Editor. It is a Rust-based kernel targeting ARM64 (aarch64-unknown-none-softfloat). It aims to run ios 5 ARMv7 userspace binaries.

## Build and Test

### Prerequisites

- Rust nightly toolchain with `aarch64-unknown-none-softfloat` target
- QEMU (qemu-system-aarch64) for emulation
- `disk.img` file in the project root

### Building the Kernel

```bash
cargo kbuild
```

This runs:
```
cargo build --package zedos-kernel --target aarch64-unknown-none-softfloat -Zbuild-std=core,alloc
```

### Running in QEMU

```bash
cargo krun
```

This runs:
```
cargo run --package zedos-kernel --target aarch64-unknown-none-softfloat -Zbuild-std=core,alloc
```

QEMU is configured with:
- Machine: `virt` (highmem=off)
- CPU: `cortex-a57`
- Cores: 4
- Memory: 2GB
- Output: Nongraphic (serial console)
- Storage: `disk.img` via virtio-blk

### Full Workflow

```bash
# Make the rootfs disk
./make_disk.sh

# Build the kernel
cargo kbuild

# Run in QEMU
cargo krun
```

## Workspace Structure

- `kernel/` - Main kernel source code
- `libs/` - Shared libraries
- `tools/` - Utility tools
- `rootfs/` - Root filesystem
