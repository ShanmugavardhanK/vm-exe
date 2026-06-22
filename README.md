# wasm-vm-executor-rs

VM wasmer 2.2 executor + specialized C API to be used from Go.

## Building

### Native (canonical path)

Run `make capi` in the root to get the host-platform binary plus the C
header. Per-platform make targets (`capi-linux-amd64`, `capi-linux-arm`,
`capi-osx-amd64`, `capi-osx-arm`) handle the soname / install_name
patching on each respective host. CI builds each artifact on its native
runner — see [.github/workflows/libvmexeccapi-build.yml](.github/workflows/libvmexeccapi-build.yml)
and [libvmexeccapi-build-linux-arm64.yml](.github/workflows/libvmexeccapi-build-linux-arm64.yml).

### Cross-build macOS dylibs from Linux

When a macOS host or runner isn't available, dylibs can be cross-compiled
from a Linux box using osxcross. See
[`scripts/README.md`](scripts/README.md) for setup + usage of
[`scripts/build-dylib-via-osxcross.sh`](scripts/build-dylib-via-osxcross.sh).

Releases tagged for end users should still ship the canonical
macOS-built dylibs; the cross-build path is for internal dev / CI / soak
use, with mandatory final validation on real macOS before any release.
