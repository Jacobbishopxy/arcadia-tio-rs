# Release notes

## 0.2.0 — public Rust wrapper source release

Tag: `0.2.0`
Commit: `3071a41`

### Scope

This is a source-only release of the public Rust wrapper workspace:

- `arcadia-tio-ocb-core` — C-ABI-free generic OCB selected-snapshot reader,
  read-planning, row-group visitor, reusable-buffer visitor, fixed-binary
  projection, attribution, and generic certification-substrate APIs.
- `arcadia-tio-sys` / `arcadia-tio-rs` — C-ABI-backed raw/safe wrapper source
  crates for consumers that supply an operator-approved native
  `arcadia_tio_capi` library.

### OCB-core guidance

- The OCB-core boundary stays generic: no channel, BizIndex, fixed-ingress,
  compact-L2, replay, order-book, or market-data semantics are defined upstream.
- Certification identity values are deterministic compatibility identifiers
  under `ocb.generic.crc32c.v1`, not cryptographic file digests.
- Downstream payload-only runtime use should be manifest-gated and fail closed by
  comparing the selected snapshot fingerprint, root/previous-root generation,
  selected row-group ids/base rows/counts, selected chunk summaries/checksums,
  selected compressed/uncompressed byte totals, plan report, and
  `selected_chunk_fingerprint`.
- For row-group-coalesced reads, build one plan, union the needed plan-local
  row-group ids, execute `read_plan_row_groups(...)` or
  `visit_plan_row_groups_into_with_attribution(...)`, and demultiplex in the
  application.

### Non-goals

This release does not publish crates.io packages, native libraries, signed
artifacts, package-manager/system installs, benchmark evidence, storage/capacity
claims, or production/default runtime readiness.

### Validation summary

Maintainer validation before tagging included:

- `cargo make ci` in the public workspace with a locally supplied native C ABI
  library;
- `cargo test -p arcadia-tio-ocb-core`;
- `cargo check -p arcadia-tio-ocb-core --examples`;
- `cargo make test-core-reader-no-cabi`;
- downstream temporary pin smoke against `arcadia-lob-player-runtime` with
  `mock-live-ocb-core-reader`.
