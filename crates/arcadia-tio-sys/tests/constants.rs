use std::ffi::c_int;
use std::mem::{align_of, size_of};

use arcadia_tio_sys::*;

const _: () = {
    assert!(size_of::<ArcadiaTioDType>() == size_of::<c_int>());
    assert!(size_of::<ArcadiaTioErrorCode>() == size_of::<c_int>());
    assert!(size_of::<ArcadiaTioV4PreciseAccountingField>() == size_of::<c_int>());
    assert!(ARCADIA_TIO_DTYPE_F32 == 0);
    assert!(ARCADIA_TIO_DTYPE_F64 == 1);
    assert!(ARCADIA_TIO_DTYPE_I32 == 2);
    assert!(ARCADIA_TIO_DTYPE_I64 == 3);
    assert!(ARCADIA_TIO_ERROR_OK == 0);
    assert!(ARCADIA_TIO_AXIS_TIME == 0);
    assert!(ARCADIA_TIO_AXIS_SYMBOL == 1);
    assert!(ARCADIA_TIO_COORDINATE_DTYPE_I32 == 0);
    assert!(ARCADIA_TIO_COORDINATE_KIND_DATE == 2);
    assert!(ARCADIA_TIO_COORDINATE_ENCODING_DATE_YYYYMMDD == 2);
    assert!(ARCADIA_TIO_COORDINATE_STORAGE_INLINE == 0);
    assert!(ARCADIA_TIO_HEADER_PROFILE_STREAMING == 0);
    assert!(ARCADIA_TIO_ENTRY_SELECTOR_ALL == 0);
    assert!(ARCADIA_TIO_ENTRY_SELECTOR_RANGE == 1);
    assert!(ARCADIA_TIO_ENTRY_SELECTOR_TAKE == 2);
    assert!(ARCADIA_TIO_READ_EXECUTION_SERIAL == 0);
    assert!(ARCADIA_TIO_READ_EXECUTION_PARALLEL_THREADS == 1);
    assert!(ARCADIA_TIO_READ_SHAPE_POLICY_FILE_ENVELOPE == 0);
    assert!(ARCADIA_TIO_READ_SHAPE_POLICY_EXPLICIT_UNIVERSE == 6);
    assert!(ARCADIA_TIO_READ_SHAPE_POLICY_EXPLICIT_UNIVERSE_AND_EXTENTS == 7);
    assert!(ARCADIA_TIO_AXIS_IDENTITY_EXTENT_ONLY == 0);
    assert!(ARCADIA_TIO_AXIS_IDENTITY_UNIVERSE_AWARE == 1);
    assert!(ARCADIA_TIO_HISTORICAL_QUERY_SOURCE_RETAINED_VISIBLE_COMMIT == 0);
    assert!(ARCADIA_TIO_COMPACTION_COPY_LIVE == 0);
    assert!(ARCADIA_TIO_COMPACTION_REBLOCK == 1);
    assert!(ARCADIA_TIO_REFORM_TARGET_PRESERVE_FAMILY == 0);
    assert!(ARCADIA_TIO_REFORM_TARGET_WHOLE_APPEND_UNIT == 1);
    assert!(ARCADIA_TIO_REFORM_TARGET_REGULAR_CHUNKED == 2);
    assert!(ARCADIA_TIO_V4_REPORT_COMPLETE == 0);
    assert!(ARCADIA_TIO_V4_REPORT_UNSUPPORTED == 1);
    assert!(ARCADIA_TIO_V4_REPORT_UNKNOWN == 2);
    assert!(ARCADIA_TIO_V4_COMPACTION_POLICY_COMPACT_TO_CURRENT_STATE == 0);
    assert!(ARCADIA_TIO_V4_PRECISE_ACCOUNTING_UNREACHABLE_BYTES == 0);
    assert!(ARCADIA_TIO_V4_PRECISE_ACCOUNTING_RETAINED_HISTORY_REQUIRED_BYTES == 1);
    assert!(ARCADIA_TIO_V4_PRECISE_ACCOUNTING_POPPED_SKIPPED_BYTES == 2);
    assert!(ARCADIA_TIO_V4_PRECISE_ACCOUNTING_RECLAIMABLE_BYTES == 3);
    assert!(ARCADIA_TIO_V4_RETAINED_HISTORY_RETAIN_LAST == 0);
    assert!(ARCADIA_TIO_STORAGE_BALANCED == 0);
    assert!(ARCADIA_TIO_STORAGE_ACCESS_REMOTE_RANGE_READ == 1);
    assert!(ARCADIA_TIO_OPEN_PATTERN_METADATA_HOT == 0);
    assert!(ARCADIA_TIO_FILE_POPULATION_FEW_LONG_LIVED == 0);
    assert!(ARCADIA_TIO_METADATA_STABILITY_STABLE == 0);
};

#[test]
fn representative_raw_layouts_are_pointer_compatible() {
    assert_eq!(align_of::<ArcadiaTioTensor>(), align_of::<usize>());
    assert_eq!(
        align_of::<ArcadiaTioAxisCoordinateInput>(),
        align_of::<usize>()
    );

    #[cfg(target_pointer_width = "64")]
    {
        assert_eq!(size_of::<ArcadiaTioAxisCoordinateInput>(), 120);
        assert_eq!(size_of::<ArcadiaTioEntrySelector>(), 32);
        assert_eq!(size_of::<ArcadiaTioChunkKey>(), 16);
        assert_eq!(size_of::<ArcadiaTioReadShapePolicyOptions>(), 72);
        assert_eq!(size_of::<ArcadiaTioReadWithOptionsOptions>(), 32);
        assert_eq!(size_of::<ArcadiaTioHistoricalReadWithOptionsOptions>(), 32);
        assert_eq!(size_of::<ArcadiaTioReadWithShapePolicyOptions>(), 104);
        assert_eq!(
            size_of::<ArcadiaTioHistoricalReadWithShapePolicyOptions>(),
            104
        );
        assert_eq!(size_of::<ArcadiaTioCreateWithUniverseOptions>(), 32);
        assert_eq!(size_of::<ArcadiaTioAppendWithUniverseOptions>(), 48);
        assert_eq!(size_of::<ArcadiaTioCompactionMode>(), 8);
        assert_eq!(size_of::<ArcadiaTioCompactionStats>(), 32);
        assert_eq!(size_of::<ArcadiaTioReformOptions>(), 40);
        assert_eq!(size_of::<ArcadiaTioReformReport>(), 40);
        assert_eq!(size_of::<ArcadiaTioV4PreciseAccountingOptions>(), 24);
        assert_eq!(size_of::<ArcadiaTioV4OmittedPreciseAccountingField>(), 32);
        assert_eq!(size_of::<ArcadiaTioV4PreciseAccountingBytes>(), 112);
        assert_eq!(size_of::<ArcadiaTioV4CurrentHeadBytes>(), 40);
        assert_eq!(size_of::<ArcadiaTioV4AuditBytes>(), 32);
        assert_eq!(size_of::<ArcadiaTioV4PayloadReuseBytes>(), 16);
        assert_eq!(size_of::<ArcadiaTioV4SupersededBytes>(), 32);
        assert_eq!(size_of::<ArcadiaTioV4DiagnosticsReport>(), 176);
        assert_eq!(size_of::<ArcadiaTioV4DiagnosticsPreciseReport>(), 280);
        assert_eq!(size_of::<ArcadiaTioV4CompactionAnalysisReport>(), 88);
        assert_eq!(
            size_of::<ArcadiaTioV4CompactionAnalysisPreciseReport>(),
            192
        );
        assert_eq!(
            size_of::<ArcadiaTioV4RetainedHistoryCompactionOptions>(),
            24
        );
        assert_eq!(
            size_of::<ArcadiaTioV4RetainedHistoryCompactionReport>(),
            104
        );
        assert_eq!(
            size_of::<ArcadiaTioV4RetainedHistoryCompactionPreciseReport>(),
            208
        );
        assert_eq!(size_of::<ArcadiaTioAutoCompactionConfig>(), 40);
        assert_eq!(size_of::<ArcadiaTioCompactionState>(), 16);
    }

    #[cfg(target_pointer_width = "32")]
    {
        assert!(size_of::<ArcadiaTioAxisCoordinateInput>() >= 72);
        assert_eq!(size_of::<ArcadiaTioChunkKey>(), 8);
    }
}
