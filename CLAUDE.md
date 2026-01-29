# CLAUDE.md

This file provides guidance for interacting with the GravityOS codebase.

## Project Overview

GravityOS is a Rust-based operating system and kernel targeting ARM64 (aarch64-unknown-none-softfloat). It aims to run ios 5 ARMv7 userspace binaries.

## Build and Test

### Prerequisites

- Nix

### Building the Kernel

```bash
nix build -v .#kernel
```

### Running the Kernel

```bash
nix run
```

## Workspace Structure

- `kernel/` - Main kernel source code
- `libs/` - Shared libraries
- `tools/` - Utility tools
- `rootfs/` - Root filesystem
