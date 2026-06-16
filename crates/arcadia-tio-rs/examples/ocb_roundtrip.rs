use std::fs;

use arcadia_tio_rs::ocb::{
    self, ColumnBundleFile, LogicalKind, NullOrder, OrderingDirection, PhysicalType,
    PrimitiveValues, Projection, ReadRequest, WriteColumn, WriteColumnChunk, WriteOrderingKey,
    WriteRowGroup, WriteSpec,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::current_dir()?.join("target/ocb-roundtrip-example.ocb");
    let _ = fs::remove_file(&path);

    let spec = WriteSpec {
        columns: vec![
            WriteColumn {
                name: "sequence_key".to_string(),
                physical_type: PhysicalType::I64,
                logical_kind: LogicalKind::OpaqueKey,
                dictionary_id: None,
                scale: 0,
                nullable: false,
            },
            WriteColumn {
                name: "metric".to_string(),
                physical_type: PhysicalType::F64,
                logical_kind: LogicalKind::Plain,
                dictionary_id: None,
                scale: 0,
                nullable: false,
            },
        ],
        dictionaries: Vec::new(),
        row_groups: vec![WriteRowGroup {
            columns: vec![
                WriteColumnChunk {
                    column_id: 0,
                    values: PrimitiveValues::I64(vec![1, 2, 3]),
                    validity: None,
                },
                WriteColumnChunk {
                    column_id: 1,
                    values: PrimitiveValues::F64(vec![10.0, 20.0, 30.0]),
                    validity: None,
                },
            ],
        }],
        ordering_keys: vec![WriteOrderingKey {
            column_id: 0,
            direction: OrderingDirection::Ascending,
            null_order: NullOrder::NoNulls,
        }],
    };

    ocb::create(&path, &spec)?;
    let file = ColumnBundleFile::open(&path)?;
    let metadata = file.metadata()?;
    println!("{} rows in {}", metadata.row_count, metadata.format_name);

    let read = file.read_batches(&ReadRequest {
        projection: Projection::Names(vec!["metric".to_string()]),
        ..ReadRequest::default()
    })?;
    println!("{} batch(es)", read.batches.len());

    let _ = fs::remove_file(path);
    Ok(())
}
