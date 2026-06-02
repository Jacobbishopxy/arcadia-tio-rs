//! Public Rust current coordinate first-surface tutorial.
//!
//! This example demonstrates the bounded public Rust current coordinate surface:
//! inline numeric values, fixed-width ASCII text, dictionary codes, unavailable
//! external-reference summaries, visible lookup status records, append-axis
//! coordinates, and no-partial-publication failures. It does not dereference
//! external references, infer variable-length string semantics, or treat
//! optional indexes as coordinate truth.

use std::fs;
use std::path::{Path, PathBuf};
use std::process;
use std::time::{SystemTime, UNIX_EPOCH};

use arcadia_tio_rs::{
    AppendCoordinateBatch, AppendCoordinateEntry, AxisCoordinateInput, AxisKind,
    CoordinateAvailability, CoordinateCodeDType, CoordinateDType, CoordinateDictionaryEntry,
    CoordinateDictionarySummary, CoordinateEncoding, CoordinateExternalBinding,
    CoordinateFixedTextLayout, CoordinateKind, CoordinateLookupKey, CoordinateLookupResultStatus,
    CoordinateMonotonicity, CoordinateOptions, CoordinateOrdering, CoordinateSourceKind,
    CoordinateStatusCategory, CoordinateUniqueness, CoordinateValueDomain, CreateOptions, DType,
    DimSpec, ErrorCode, TensorData, TensorFile,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Step 1: keep all coordinate examples in one temporary tutorial workspace.
    let temp = TutorialTempDir::new("coordinates_full_surface")?;

    // Step 2: run independent demos for each current-coordinate domain.
    demo_numeric_and_fixed_text(temp.path())?;
    demo_dictionary_codes(temp.path())?;
    demo_external_unavailable(temp.path())?;
    demo_append_coordinates_and_atomic_failures(temp.path())?;

    println!(
        "coordinate ok: metadata, lookups, external status, append coordinates, and atomic failures passed in {}",
        temp.path().display()
    );
    Ok(())
}

fn demo_numeric_and_fixed_text(root: &Path) -> Result<(), Box<dyn std::error::Error>> {
    // Step 1: create one file with numeric and fixed-width text coordinates.
    let path = root.join("coordinate_numeric_fixed_text.tio");
    let options = CreateOptions::streaming(
        DType::F32,
        vec![
            DimSpec::new(AxisKind::Time, 0).with_name("time"),
            DimSpec::new(AxisKind::Symbol, 2).with_name("symbol"),
            DimSpec::new(AxisKind::Channel, 2).with_name("channel"),
        ],
        0,
    );
    let coordinates = vec![
        AxisCoordinateInput::inline_i32(1, vec![10, 20])
            .with_descriptor_id("symbol-id")
            .with_name("symbol_id")
            .with_kind(CoordinateKind::LabelId)
            .with_required(true)
            .with_ordering(CoordinateOrdering {
                sorted: arcadia_tio_rs::CoordinateSortedness::Ascending,
                monotonicity: CoordinateMonotonicity::StrictlyIncreasing,
                uniqueness: CoordinateUniqueness::Unique,
            }),
        AxisCoordinateInput::fixed_text_ascii(2, 4, ["BID", "ASK"])?
            .with_descriptor_id("channel-code")
            .with_name("channel_code")
            .with_kind(CoordinateKind::LabelId)
            .with_required(true),
    ];

    // Step 2: create the tensor with coordinate descriptors and append payload values.
    let mut file = TensorFile::create_with_coordinates(
        &path,
        options,
        &coordinates,
        CoordinateOptions::default(),
    )?;
    file.append_f32(&[1.0, 2.0, 3.0, 4.0], &[1, 2, 2])?;

    // Step 3: inspect descriptor metadata and status fields.
    let meta = file.coordinate_metadata()?;
    assert_eq!(meta.len(), 2);
    assert_eq!(meta[0].descriptor_id.as_deref(), Some("symbol-id"));
    assert_eq!(meta[0].value_domain, CoordinateValueDomain::InlineNumeric);
    assert_eq!(meta[0].availability, CoordinateAvailability::Available);
    assert_eq!(meta[0].status_category, CoordinateStatusCategory::Ok);
    assert_eq!(meta[1].descriptor_id.as_deref(), Some("channel-code"));
    assert_eq!(meta[1].value_domain, CoordinateValueDomain::FixedText);
    assert_eq!(meta[1].fixed_text.width, 4);

    // Step 4: read coordinate values for numeric and fixed-text domains.
    let numeric_values = file.read_coordinate_axis(1, CoordinateOptions::default())?;
    assert_eq!(
        numeric_values.value_domain,
        CoordinateValueDomain::InlineNumeric
    );
    assert_eq!(numeric_values.numeric_dtype, CoordinateDType::I32);
    assert_eq!(numeric_values.element_size, std::mem::size_of::<i32>());
    assert_eq!(numeric_values.len, 2);
    assert_eq!(numeric_values.data, i32_bytes(&[10, 20]));

    let fixed_values = file.read_coordinate_axis(2, CoordinateOptions::default())?;
    assert_eq!(fixed_values.value_domain, CoordinateValueDomain::FixedText);
    assert_eq!(fixed_values.fixed_text_width, 4);
    assert_eq!(fixed_values.len, 2);
    assert_eq!(fixed_values.data, b"BID ASK ".to_vec());

    // Step 5: run exact/range lookups and preserve ordinary status outcomes.
    let lookup_options = CoordinateOptions::authoritative_scan();
    let numeric_exact = file.coordinate_lookup(1, &CoordinateLookupKey::i32(20), lookup_options)?;
    assert_eq!(numeric_exact.status, CoordinateLookupResultStatus::Unique);
    assert_eq!(numeric_exact.unique_position(), Some(1));

    let numeric_range = file.coordinate_lookup_range(
        1,
        &CoordinateLookupKey::i32(10),
        &CoordinateLookupKey::i32(21),
        lookup_options,
    )?;
    assert_eq!(numeric_range.status, CoordinateLookupResultStatus::Range);
    assert_eq!(numeric_range.range(), Some(0..2));

    let fixed_exact = file.coordinate_lookup(
        2,
        &CoordinateLookupKey::fixed_text_ascii("ASK", 4)?,
        lookup_options,
    )?;
    assert_eq!(fixed_exact.status, CoordinateLookupResultStatus::Unique);
    assert_eq!(fixed_exact.unique_position(), Some(1));

    let fixed_range = file.coordinate_lookup_range(
        2,
        &CoordinateLookupKey::fixed_text_ascii("BID", 4)?,
        &CoordinateLookupKey::fixed_text_ascii("BIE", 4)?,
        lookup_options,
    )?;
    assert_eq!(fixed_range.status, CoordinateLookupResultStatus::Range);
    assert_eq!(fixed_range.range(), Some(0..1));

    let missing = file.coordinate_lookup(1, &CoordinateLookupKey::i32(99), lookup_options)?;
    assert_eq!(missing.status, CoordinateLookupResultStatus::Missing);
    assert_eq!(missing.status_category, CoordinateStatusCategory::Ok);

    let domain_mismatch =
        file.coordinate_lookup(2, &CoordinateLookupKey::i64(10), lookup_options)?;
    assert_eq!(
        domain_mismatch.status_category,
        CoordinateStatusCategory::LookupDomainMismatch
    );
    assert!(domain_mismatch.is_error() || domain_mismatch.is_unsupported());

    Ok(())
}

fn demo_dictionary_codes(root: &Path) -> Result<(), Box<dyn std::error::Error>> {
    // Step 1: declare a dictionary-coded coordinate axis with create-time entries.
    let path = root.join("coordinate_dictionary.tio");
    let options = CreateOptions::streaming(
        DType::F64,
        vec![
            DimSpec::new(AxisKind::Time, 0).with_name("time"),
            DimSpec::new(AxisKind::Symbol, 2).with_name("symbol"),
        ],
        0,
    );
    let dictionary_summary = CoordinateDictionarySummary::new(CoordinateCodeDType::U16)
        .with_dictionary_id("symbol-dict")
        .with_revision(7)
        .with_content_id("symbol-dict-content");
    let dictionary_entries = vec![
        CoordinateDictionaryEntry::new(1, Some("AAPL".to_string()), Some("AAPL".to_string())),
        CoordinateDictionaryEntry::new(2, Some("MSFT".to_string()), Some("MSFT".to_string())),
    ];
    let coordinates = vec![
        AxisCoordinateInput::dictionary_codes_u16(
            1,
            vec![1, 2],
            CoordinateFixedTextLayout::ascii_right_space_padded(4)?,
            dictionary_summary,
            dictionary_entries,
        )?
        .with_descriptor_id("symbol-dictionary-code")
        .with_name("symbol_code")
        .with_required(true),
    ];

    // Step 2: create the file and append one payload row.
    let mut file = TensorFile::create_with_coordinates(
        &path,
        options,
        &coordinates,
        CoordinateOptions::default(),
    )?;
    file.append_f64(&[1.5, 2.5], &[1, 2])?;

    // Step 3: verify dictionary metadata and code values.
    let meta = file.coordinate_metadata()?;
    assert_eq!(meta.len(), 1);
    assert_eq!(meta[0].value_domain, CoordinateValueDomain::DictionaryCode);
    assert_eq!(
        meta[0].dictionary.dictionary_id.as_deref(),
        Some("symbol-dict")
    );
    assert_eq!(meta[0].dictionary.revision, 7);
    assert_eq!(meta[0].dictionary.entry_count, 2);

    let code_values = file.read_coordinate_axis(1, CoordinateOptions::default())?;
    assert_eq!(
        code_values.value_domain,
        CoordinateValueDomain::DictionaryCode
    );
    assert_eq!(code_values.code_dtype, CoordinateCodeDType::U16);
    assert_eq!(code_values.data, u16_bytes(&[1, 2]));

    // Step 4: request dictionary entries explicitly when reading dictionary metadata.
    let dictionary = file.coordinate_dictionary(
        1,
        CoordinateOptions {
            include_dictionary_entries: true,
            ..CoordinateOptions::default()
        },
    )?;
    assert_eq!(dictionary.status_category, CoordinateStatusCategory::Ok);
    assert_eq!(dictionary.entries.len(), 2);
    assert_eq!(dictionary.entries[0].stable_id.as_deref(), Some("AAPL"));
    assert_eq!(dictionary.entries[1].display_label.as_deref(), Some("MSFT"));

    // Step 5: look up by code, stable id, and display label.
    let lookup_options = CoordinateOptions::authoritative_scan();
    let code_lookup =
        file.coordinate_lookup(1, &CoordinateLookupKey::dictionary_code(2), lookup_options)?;
    assert_eq!(code_lookup.status, CoordinateLookupResultStatus::Unique);
    assert_eq!(code_lookup.unique_position(), Some(1));

    let stable_lookup =
        file.coordinate_lookup(1, &CoordinateLookupKey::stable_id("AAPL"), lookup_options)?;
    assert_eq!(stable_lookup.status, CoordinateLookupResultStatus::Unique);
    assert_eq!(stable_lookup.unique_position(), Some(0));

    let label_lookup = file.coordinate_lookup(
        1,
        &CoordinateLookupKey::display_label("MSFT"),
        lookup_options,
    )?;
    assert_eq!(label_lookup.status, CoordinateLookupResultStatus::Unique);
    assert_eq!(label_lookup.unique_position(), Some(1));

    Ok(())
}

fn demo_external_unavailable(root: &Path) -> Result<(), Box<dyn std::error::Error>> {
    // Step 1: declare an external-reference summary without dereferencing it.
    let path = root.join("coordinate_external_unavailable.tio");
    let options = CreateOptions::streaming(
        DType::F32,
        vec![
            DimSpec::new(AxisKind::Time, 0).with_name("time"),
            DimSpec::new(AxisKind::Symbol, 2).with_name("symbol"),
        ],
        0,
    );
    let external_binding = CoordinateExternalBinding::metadata_only(
        CoordinateSourceKind::SameFileObject,
        Some("tutorial_external_symbol_source".to_string()),
        Some("symbol coordinate object".to_string()),
        CoordinateValueDomain::FixedText,
        2,
    );
    let coordinates = vec![
        AxisCoordinateInput::external_reference_fixed_text(
            1,
            external_binding,
            CoordinateFixedTextLayout::ascii_right_space_padded(4)?,
        )?
        .with_descriptor_id("external-symbol")
        .with_name("external_symbol"),
    ];

    // Step 2: create a payload file; the external coordinate stays metadata-only.
    let mut file = TensorFile::create_with_coordinates(
        &path,
        options,
        &coordinates,
        CoordinateOptions::default(),
    )?;
    file.append_f32(&[3.0, 4.0], &[1, 2])?;

    // Step 3: verify unavailable-but-OK status is surfaced explicitly.
    let meta = file.coordinate_metadata()?;
    assert_eq!(meta.len(), 1);
    assert_eq!(
        meta[0].value_domain,
        CoordinateValueDomain::ExternalReference
    );
    assert_eq!(meta[0].availability, CoordinateAvailability::Unavailable);
    assert_eq!(meta[0].status_category, CoordinateStatusCategory::Ok);
    assert_eq!(
        meta[0].external_binding.logical_id.as_deref(),
        Some("tutorial_external_symbol_source")
    );

    let values = file.read_coordinate_axis(1, CoordinateOptions::default())?;
    assert_ne!(values.availability, CoordinateAvailability::Available);
    assert_eq!(values.status_category, CoordinateStatusCategory::Ok);
    assert!(values.data.is_empty());

    // Step 4: lookup returns an unavailable status instead of resolving external values.
    let unavailable_lookup = file.coordinate_lookup(
        1,
        &CoordinateLookupKey::fixed_text_ascii("AAPL", 4)?,
        CoordinateOptions::default(),
    )?;
    assert_eq!(
        unavailable_lookup.status,
        CoordinateLookupResultStatus::Unavailable
    );
    assert_eq!(
        unavailable_lookup.availability,
        CoordinateAvailability::Unavailable
    );

    Ok(())
}

fn demo_append_coordinates_and_atomic_failures(
    root: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    // Step 1: declare a required append-axis coordinate sequence.
    let path = root.join("coordinate_append.tio");
    let options = CreateOptions::streaming(
        DType::F32,
        vec![
            DimSpec::new(AxisKind::Time, 0).with_name("time"),
            DimSpec::new(AxisKind::Channel, 2).with_name("channel"),
        ],
        0,
    );
    let coordinates = vec![
        AxisCoordinateInput::append_numeric_i32(0)
            .with_descriptor_id("append-day")
            .with_name("append_day")
            .with_kind(CoordinateKind::Date)
            .with_numeric_encoding(CoordinateEncoding::DateYyyymmdd)
            .with_required(true)
            .with_ordering(CoordinateOrdering {
                sorted: arcadia_tio_rs::CoordinateSortedness::Ascending,
                monotonicity: CoordinateMonotonicity::StrictlyIncreasing,
                uniqueness: CoordinateUniqueness::Unique,
            }),
    ];

    // Step 2: append payload and matching coordinate values atomically.
    let mut file = TensorFile::create_with_coordinates(
        &path,
        options,
        &coordinates,
        CoordinateOptions::default(),
    )?;
    let good_batch = AppendCoordinateBatch::new(vec![
        AppendCoordinateEntry::i32(0, vec![20260531, 20260601])
            .with_descriptor_id("append-day")
            .with_numeric_encoding(CoordinateEncoding::DateYyyymmdd),
    ]);
    let range = file.append_f32_with_coordinates(&[1.0, 2.0, 3.0, 4.0], &[2, 2], &good_batch)?;
    assert_eq!((range.start, range.end), (0, 2));
    assert_eq!(file.dim_lens()?, vec![2, 2]);
    assert_eq!(
        file.read_all()?.data,
        TensorData::F32(vec![1.0, 2.0, 3.0, 4.0])
    );

    // Step 3: snapshot state before exercising failing append cases.
    let before_dims = file.dim_lens()?;
    let before_payload = file.read_all()?;
    let before_meta = file.coordinate_metadata()?;
    let before_values = file.read_coordinate_axis(0, CoordinateOptions::default())?;

    // Step 4: missing required coordinates reject before publishing payload.
    let missing = file
        .append_f32_with_coordinates(
            &[5.0, 6.0, 7.0, 8.0],
            &[2, 2],
            &AppendCoordinateBatch::empty(),
        )
        .expect_err("missing required append coordinate should fail");
    assert_eq!(missing.code(), ErrorCode::InvalidArgument);

    // Step 5: coordinate counts must match the appended payload extent.
    let wrong_count_batch = AppendCoordinateBatch::new(vec![
        AppendCoordinateEntry::i32(0, vec![20260602])
            .with_descriptor_id("append-day")
            .with_numeric_encoding(CoordinateEncoding::DateYyyymmdd),
    ]);
    let wrong_count = file
        .append_f32_with_coordinates(&[5.0, 6.0, 7.0, 8.0], &[2, 2], &wrong_count_batch)
        .expect_err("wrong-count append coordinate should fail");
    assert_eq!(wrong_count.code(), ErrorCode::InvalidArgument);

    // Step 6: failed coordinate appends preserve dimensions, payload, metadata, and values.
    assert_eq!(file.dim_lens()?, before_dims);
    assert_eq!(file.read_all()?, before_payload);
    assert_eq!(file.coordinate_metadata()?, before_meta);
    assert_eq!(
        file.read_coordinate_axis(0, CoordinateOptions::default())?,
        before_values
    );

    Ok(())
}

fn i32_bytes(values: &[i32]) -> Vec<u8> {
    values
        .iter()
        .flat_map(|value| value.to_ne_bytes())
        .collect()
}

fn u16_bytes(values: &[u16]) -> Vec<u8> {
    values
        .iter()
        .flat_map(|value| value.to_ne_bytes())
        .collect()
}

struct TutorialTempDir {
    path: PathBuf,
}

impl TutorialTempDir {
    fn new(label: &str) -> std::io::Result<Self> {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or(0);
        let path = std::env::temp_dir().join(format!(
            "arcadia_tio_rust_tutorial_{label}_{}_{}",
            process::id(),
            nanos
        ));
        fs::create_dir(&path)?;
        Ok(Self { path })
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TutorialTempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}
