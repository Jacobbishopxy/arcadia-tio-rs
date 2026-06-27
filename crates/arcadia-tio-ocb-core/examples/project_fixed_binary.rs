use std::{env, path::PathBuf};

use arcadia_tio_ocb_core::{
    ColumnBundleFile, ColumnBundleReadCursorOptions, ColumnBundleReadOptions,
    ColumnBundleReadRequest, ColumnBundleVisitControl, ColumnProjection, FixedBinaryFieldType,
    FixedBinaryProjectedField, FixedBinaryRecordProjection, Result,
};

fn main() -> Result<()> {
    let mut args = env::args_os().skip(1);
    let Some(path) = args.next().map(PathBuf::from) else {
        eprintln!(
            "usage: cargo run -p arcadia-tio-ocb-core --example project_fixed_binary -- \\\n             <file.ocb> <fixed-binary-column> <record-width>"
        );
        return Ok(());
    };
    let column_name = args
        .next()
        .and_then(|value| value.into_string().ok())
        .unwrap_or_else(|| "payload".to_string());
    let record_width = args
        .next()
        .and_then(|value| value.into_string().ok())
        .and_then(|value| value.parse::<u32>().ok())
        .unwrap_or(16);

    let file = ColumnBundleFile::open(&path)?;
    let request = ColumnBundleReadRequest {
        projection: ColumnProjection::names([column_name.as_str()]),
        predicates: Vec::new(),
        options: ColumnBundleReadOptions::parallel(4),
    };
    let plan = file.plan_read(&request)?;
    let row_group_ids = plan.row_group_ids.clone();

    // Generic field layout chosen by the caller. OCB does not attach channel,
    // BizIndex, order-book, or market-data semantics to these bytes.
    let projection = FixedBinaryRecordProjection::by_column_name(column_name, record_width)
        .field(FixedBinaryProjectedField::new(0, FixedBinaryFieldType::U8).with_name("tag"))
        .field(FixedBinaryProjectedField::new(4, FixedBinaryFieldType::I32Le).with_name("key"))
        .field(
            FixedBinaryProjectedField::new(8, FixedBinaryFieldType::I64Le).with_name("sequence"),
        );

    let mut reusable = file.reusable_buffer_pool_for_plan(&plan, 4, false)?;
    let mut projection_buffer = file.fixed_binary_projection_buffer_for_plan(&plan, &projection)?;
    let report = file.visit_plan_row_groups_project_fixed_binary_with_attribution(
        &plan,
        &row_group_ids,
        ColumnBundleReadCursorOptions {
            max_in_flight_row_groups: 4,
            ordered: true,
        },
        &mut reusable,
        &projection,
        &mut projection_buffer,
        |batch, projected| {
            let tags = projected.field_by_name("tag")?.values.as_u8()?;
            let keys = projected.field_by_name("key")?.values.as_i32()?;
            let sequences = projected.field_by_name("sequence")?.values.as_i64()?;
            println!(
                "row_group={} rows={} first_tag={:?} first_key={:?} first_sequence={:?}",
                batch.row_group_id(),
                batch.row_count(),
                tags.first(),
                keys.first(),
                sequences.first()
            );
            Ok(ColumnBundleVisitControl::Continue)
        },
    )?;

    println!(
        "batches={} rows={} max_in_flight={} fixed_payload_decode_ns={} copy_materialization_ns={} callback_wall_ns={}",
        report.cursor_report.batches_yielded,
        report.cursor_report.rows_yielded,
        report.cursor_report.max_in_flight_row_groups_observed,
        report.attribution.fixed_payload_decode_ns,
        report.attribution.copy_materialization_ns,
        report.attribution.callback_wall_ns
    );
    Ok(())
}
