use arcadia_tio_rs::{AxisKind, CreateOptions, DType, DimSpec, TensorData, TensorFile};

fn main() -> arcadia_tio_rs::Result<()> {
    let path =
        std::env::temp_dir().join(format!("arcadia-tio-rs-example-{}.tio", std::process::id()));
    let options = CreateOptions::streaming(DType::F64, vec![DimSpec::new(AxisKind::Time, 0)], 0);

    {
        let mut file = TensorFile::create(&path, options)?;
        file.append_f64(&[1.0, 2.0, 3.0], &[3])?;
    }

    let file = TensorFile::open(&path)?;
    let tensor = file.read_all()?;
    assert_eq!(tensor.shape, vec![3]);
    assert_eq!(tensor.data, TensorData::F64(vec![1.0, 2.0, 3.0]));
    drop(file);
    let _ = std::fs::remove_file(path);
    Ok(())
}
