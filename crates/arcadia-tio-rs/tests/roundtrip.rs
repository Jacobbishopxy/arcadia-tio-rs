use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

use arcadia_tio_rs::{
    AxisKind, CoordinateDType, CoordinateEncoding, CoordinateKind, CoordinateMonotonicity,
    CompressionConfig, CoordinateOrdering, CoordinateSpec, CoordinateStorage, CoordinateStorageKind,
    CoordinateUniqueness, CoordinateValidationStatus, CoordinateValues, CreateOptions, DType,
    DimSpec, TensorData, TensorFile,
};

#[test]
fn safe_wrapper_roundtrips_f64_with_metadata_and_coordinates() {
    let path = unique_path("safe-wrapper-f64.tio");
    let dims = vec![
        DimSpec::new(AxisKind::Time, 0).with_name("time"),
        DimSpec::new(AxisKind::Channel, 2).with_name("channel"),
    ];
    let mut options = CreateOptions::streaming(DType::F64, dims, 0);
    options.channels = vec!["bid".to_string(), "ask".to_string()];
    options.user_kv = vec![("source".to_string(), "safe-wrapper-test".to_string())];
    options.coordinates.push(CoordinateSpec {
        axis: 1,
        name: Some("channel_id".to_string()),
        kind: CoordinateKind::LabelId,
        encoding: CoordinateEncoding::Plain,
        storage: CoordinateStorage::Inline(CoordinateValues::I32(vec![10, 20])),
        ordering: CoordinateOrdering {
            sorted: arcadia_tio_rs::CoordinateSortedness::Ascending,
            monotonicity: CoordinateMonotonicity::StrictlyIncreasing,
            uniqueness: CoordinateUniqueness::Unique,
        },
        required: true,
    });

    {
        let mut file = TensorFile::create(&path, options).expect("create through safe wrapper");
        let range = file
            .append_f64(&[1.0, 2.0, 3.0, 4.0], &[2, 2])
            .expect("append through safe wrapper");
        assert_eq!((range.start, range.end), (0, 2));
        assert_eq!(file.dtype().expect("dtype"), DType::F64);
        assert_eq!(file.dim_lens().expect("dim lens"), vec![2, 2]);
    }

    let file = TensorFile::open(&path).expect("reopen through safe wrapper");
    let tensor = file.read_all().expect("read through safe wrapper");
    assert_eq!(tensor.dtype, DType::F64);
    assert_eq!(tensor.shape, vec![2, 2]);
    assert_eq!(tensor.data, TensorData::F64(vec![1.0, 2.0, 3.0, 4.0]));

    let meta = TensorFile::load_meta(&path).expect("load metadata");
    assert_eq!(meta.dtype, DType::F64);
    assert_eq!(meta.dims.len(), 2);
    assert_eq!(meta.dims[0].name.as_deref(), Some("time"));
    assert_eq!(meta.channels.len(), 2);
    assert_eq!(meta.user_kv[0].key, "source");

    let coordinates = file.coordinate_meta().expect("coordinate metadata");
    assert_eq!(coordinates.len(), 1);
    assert_eq!(coordinates[0].axis, 1);
    assert_eq!(coordinates[0].name.as_deref(), Some("channel_id"));
    assert_eq!(coordinates[0].dtype, CoordinateDType::I32);
    assert_eq!(coordinates[0].storage_kind, CoordinateStorageKind::Inline);
    assert_eq!(
        coordinates[0].validation_status,
        CoordinateValidationStatus::Validated
    );

    let coordinate_values = file
        .read_axis_coordinates(1)
        .expect("inline coordinate values");
    assert_eq!(coordinate_values.dtype, DType::I32);
    assert_eq!(coordinate_values.shape, vec![2]);
    assert_eq!(coordinate_values.data, TensorData::I32(vec![10, 20]));

    drop(file);
    let _ = fs::remove_file(path);
}

#[test]
fn safe_wrapper_compression_option_roundtrips_f32() {
    let path = unique_path("safe-wrapper-compressed-f32.tio");
    let dims = vec![
        DimSpec::new(AxisKind::Time, 0),
        DimSpec::new(AxisKind::Symbol, 32),
    ];
    let mut options = CreateOptions::streaming(DType::F32, dims, 0);
    options.compression = Some(CompressionConfig::zstd_level(3));
    let values = vec![0.0f32; 4 * 32];
    {
        let mut file = TensorFile::create(&path, options).expect("create compressed wrapper file");
        let range = file
            .append_f32(&values, &[4, 32])
            .expect("append compressed wrapper values");
        assert_eq!((range.start, range.end), (0, 4));
    }
    let file = TensorFile::open(&path).expect("open compressed wrapper file");
    let tensor = file.read_all().expect("read compressed wrapper values");
    assert_eq!(tensor.dtype, DType::F32);
    assert_eq!(tensor.shape, vec![4, 32]);
    assert_eq!(tensor.data, TensorData::F32(values));
    drop(file);
    let _ = fs::remove_file(path);
}

#[test]
fn safe_wrapper_roundtrips_all_first_slice_numeric_dtypes() {
    roundtrip_dtype(
        "f32",
        DType::F32,
        |file| file.append_f32(&[1.5, 2.5, 3.5], &[3]),
        TensorData::F32(vec![1.5, 2.5, 3.5]),
    );
    roundtrip_dtype(
        "f64",
        DType::F64,
        |file| file.append_f64(&[1.25, 2.25, 3.25], &[3]),
        TensorData::F64(vec![1.25, 2.25, 3.25]),
    );
    roundtrip_dtype(
        "i32",
        DType::I32,
        |file| file.append_i32(&[1, 2, 3], &[3]),
        TensorData::I32(vec![1, 2, 3]),
    );
    roundtrip_dtype(
        "i64",
        DType::I64,
        |file| file.append_i64(&[10, 20, 30], &[3]),
        TensorData::I64(vec![10, 20, 30]),
    );
}

fn roundtrip_dtype(
    label: &str,
    dtype: DType,
    append: impl FnOnce(&mut TensorFile) -> arcadia_tio_rs::Result<arcadia_tio_rs::AppendRange>,
    expected: TensorData,
) {
    let path = unique_path(&format!("safe-wrapper-{label}.tio"));
    let options = CreateOptions::streaming(dtype, vec![DimSpec::new(AxisKind::Time, 0)], 0);
    {
        let mut file = TensorFile::create(&path, options).expect("create through safe wrapper");
        let range = append(&mut file).expect("append through safe wrapper");
        assert_eq!((range.start, range.end), (0, 3));
    }
    let file = TensorFile::open(&path).expect("open through safe wrapper");
    let tensor = file.read_all().expect("read through safe wrapper");
    assert_eq!(tensor.dtype, dtype);
    assert_eq!(tensor.shape, vec![3]);
    assert_eq!(tensor.data, expected);
    drop(file);
    let _ = fs::remove_file(path);
}

fn unique_path(name: &str) -> PathBuf {
    let nonce = format!("{}-{}", std::process::id(), unique_counter());
    std::env::temp_dir().join(format!("{nonce}-{name}"))
}

fn unique_counter() -> usize {
    static COUNTER: AtomicUsize = AtomicUsize::new(0);
    COUNTER.fetch_add(1, Ordering::Relaxed)
}
