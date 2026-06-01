#!/usr/bin/env bash
set -euo pipefail

# Cargo target runner for the public Rust wrapper examples/tests.
#
# Link-time discovery is handled by arcadia-tio-sys build.rs. This runner only
# mirrors the selected native-library directory into the platform runtime loader
# environment before exec'ing the compiled test/example binary.

if [[ "$#" -lt 1 ]]; then
  echo "with_native_lib.sh: expected executable path from Cargo" >&2
  exit 2
fi

script_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)"
repo_root="$(cd -- "$script_dir/../../.." && pwd -P)"
rustc_version_verbose="$(rustc -vV)"
host="$(awk '/^host:/ { print $2; exit }' <<<"$rustc_version_verbose")"
target="${CARGO_BUILD_TARGET:-${TARGET:-$host}}"

lib_dir=""
for candidate in \
  "${ARCADIA_TIO_CAPI_LIB_DIR:-}" \
  "${ARCADIA_TIO_NATIVE_LIB_DIR:-}" \
  "$repo_root/native/$target/lib" \
  "$repo_root/crates/arcadia-tio-sys/native/$target/lib"; do
  if [[ -n "$candidate" && -d "$candidate" ]]; then
    lib_dir="$candidate"
    break
  fi
done

if [[ -n "$lib_dir" ]]; then
  export ARCADIA_TIO_CAPI_LIB_DIR="$lib_dir"
  case "$(uname -s 2>/dev/null || echo unknown)" in
    Darwin*)
      export DYLD_LIBRARY_PATH="$lib_dir${DYLD_LIBRARY_PATH:+:$DYLD_LIBRARY_PATH}"
      ;;
    MINGW*|MSYS*|CYGWIN*)
      export PATH="$lib_dir${PATH:+:$PATH}"
      ;;
    *)
      export LD_LIBRARY_PATH="$lib_dir${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"
      ;;
  esac
fi

exec "$@"
