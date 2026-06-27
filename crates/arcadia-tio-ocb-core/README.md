# arcadia-tio-ocb-core

Source-visible Rust-core reader and bounded visitor APIs for Arcadia Ordered
Column Bundle (OCB) files.

This crate is intended for downstream Rust integrations that need OCB
selected-snapshot open, metadata inspection, read planning, projected/predicate
batch reads, explicit row-group visitors, reusable-buffer lower-copy visitors,
generic fixed-binary record field projection helpers, read-plan certification
summaries, and read attribution without linking the native C ABI wrapper path.

It does not provide writer APIs, C/Python bindings, `TensorFile`, market-data or
L2 semantics, native compact-L2 decode/projection, payload certification
manifests, native libraries, release artifacts, or performance/storage claims.

## Visitor contract

`ColumnBundleFile::visit_plan_row_groups_with_attribution(...)` validates the
explicit row-group ids against the supplied plan before payload reads. Unknown
and duplicate row-group ids fail closed with `ArcadiaTioErrorCode::InvalidArgument`,
`OcbFailureCause::InvalidInput`, and stable message constants:

- `OCB_READ_PLAN_SUBSET_DUPLICATE_ROW_GROUP_ERROR`
- `OCB_READ_PLAN_SUBSET_UNKNOWN_ROW_GROUP_ERROR`

Batches are yielded in original plan order, not caller subset order. Decoded
materialization is bounded by `min(max_in_flight_row_groups, effective_threads)`.
`callback_wall_ns` and `max_in_flight_row_groups_observed` are available for
visitor diagnostics.

For lower-copy reads, allocate a reusable pool with
`ColumnBundleFile::reusable_buffer_pool_for_plan(...)` and call
`visit_plan_row_groups_into(...)` or
`visit_plan_row_groups_into_with_attribution(...)`. The callback receives a
`ColumnBundleReusableBatchView<'_>` whose borrowed slices are valid only for the
callback duration and are overwritten when the pool slot is reused.

For packed fixed-width binary columns, `PrimitiveColumnValuesRef::fixed_binary_records`
and `FixedBinaryRecordView::{project_fields, project_fields_with_report}` can
decode little-endian primitive fields at caller-supplied byte offsets into
caller-owned buffers with optional projection-wall attribution. This helper is
generic; it does not add channel, BizIndex, fixed-ingress, order-book, replay, or
market-data semantics to OCB.

For fail-closed payload-only gates, `snapshot_fingerprint()` and
`read_plan_certification(...)` expose deterministic generic metadata
fingerprints, projected row-group/chunk summaries, selected payload byte totals,
and selected chunk checksum fingerprints. These are certification substrates,
not cryptographic file digests or application-semantic guarantees.
