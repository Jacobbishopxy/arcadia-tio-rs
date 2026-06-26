#![allow(dead_code)]

//! OCB/v2 metadata and object readers.

use std::collections::{BTreeMap, BTreeSet};
use std::fs::File;
use std::io::{Cursor, ErrorKind, Read, Seek, SeekFrom};
use std::path::Path;
use std::time::Duration;
use std::time::Instant;

use super::format::{
    OCB_BOOTSTRAP_MAGIC_V1, OCB_BOOTSTRAP_MAGIC_V2, OCB_BOOTSTRAP_PAGE_V1_LEN,
    OCB_BOOTSTRAP_PAGE_V2_LEN, OCB_COLUMN_CHUNK_MAGIC_V1, OCB_COLUMN_CHUNK_V1_HEADER_LEN,
    OCB_NULL_U32, OCB_ROOT_V1_LEN, OcbBodyKindV1, OcbBodyRefV2, OcbBootstrapPageV1,
    OcbBootstrapPageV2, OcbChecksumKindV1, OcbChunkCodecV1, OcbColumnChunkDescV1,
    OcbColumnChunkObjectV1, OcbColumnStatsV1, OcbDictionaryIndexV1, OcbDictionaryValuesV1,
    OcbLogicalKindV1, OcbNullabilityV1, OcbOrderingProofV1, OcbRootSlotV2, OcbRootV1, OcbRootV2,
    OcbRowGroupDescV1, OcbRowGroupIndexDeltaV1, OcbRowGroupIndexV1, OcbSchemaV1, OcbStringTableV1,
    crc32c, crc32c_finish, crc32c_init, crc32c_update,
};
use crate::{ArcadiaTioError, Result};

#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct OcbReadObjectAttribution {
    pub(crate) read_io: Duration,
    pub(crate) checksum: Duration,
    pub(crate) bytes_read: u64,
}

#[derive(Debug, Clone)]
pub(crate) struct OcbMetadataV1 {
    pub(crate) root: OcbRootV1,
    pub(crate) string_table: OcbStringTableV1,
    pub(crate) schema: OcbSchemaV1,
    pub(crate) dictionary_index: Option<OcbDictionaryIndexV1>,
    pub(crate) row_group_index: OcbRowGroupIndexV1,
    pub(crate) ordering_proof: Option<OcbOrderingProofV1>,
    pub(crate) file_len: u64,
    pub(crate) appendable: bool,
    pub(crate) root_generation: u64,
    pub(crate) previous_root_generation: Option<u64>,
}

#[derive(Debug)]
struct OcbRootCandidateV2 {
    slot: OcbRootSlotV2,
    root: OcbRootV2,
}

#[derive(Debug, Clone)]
pub(crate) struct OcbMaintenanceAnalysisV2 {
    pub(crate) file_len: u64,
    pub(crate) selected_slot_id: u16,
    pub(crate) selected_root_generation: u64,
    pub(crate) previous_root_generation: Option<u64>,
    pub(crate) selected_root_end_offset: u64,
    pub(crate) selected_snapshot_end_offset: u64,
    pub(crate) metadata: OcbMetadataV1,
    pub(crate) rejected_candidates: Vec<OcbRootCandidateDiagnosticV2>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct OcbRootCandidateDiagnosticV2 {
    pub(crate) slot_id: Option<u16>,
    pub(crate) generation: Option<u64>,
    pub(crate) message: String,
}

fn read_exact_ocb<R: Read>(reader: &mut R, buf: &mut [u8]) -> Result<()> {
    reader.read_exact(buf).map_err(|err| {
        if err.kind() == ErrorKind::UnexpectedEof {
            ArcadiaTioError::ocb_corrupt_file("OCB object is truncated")
        } else {
            ArcadiaTioError::Io(err)
        }
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum OcbOpenValidationMode {
    MetadataGraph,
    FullPayload,
}

pub(crate) fn read_metadata(path: &Path) -> Result<OcbMetadataV1> {
    read_metadata_with_validation(path, OcbOpenValidationMode::MetadataGraph)
}

pub(crate) fn read_metadata_with_validation(
    path: &Path,
    validation: OcbOpenValidationMode,
) -> Result<OcbMetadataV1> {
    let mut file = File::open(path)?;
    let file_len = file.metadata()?.len();
    if file_len < OCB_BOOTSTRAP_PAGE_V1_LEN as u64 {
        return Err(ArcadiaTioError::ocb_corrupt_file(
            "OCB file is shorter than bootstrap page",
        ));
    }

    let mut magic = [0u8; 8];
    file.seek(SeekFrom::Start(0))?;
    read_exact_ocb(&mut file, &mut magic)?;
    file.seek(SeekFrom::Start(0))?;

    match magic {
        OCB_BOOTSTRAP_MAGIC_V1 => read_metadata_v1(&mut file, file_len),
        OCB_BOOTSTRAP_MAGIC_V2 => read_metadata_v2(&mut file, file_len, validation),
        _ => Err(ArcadiaTioError::ocb_unsupported_format(
            "invalid OCB bootstrap magic",
        )),
    }
}

fn read_metadata_v1(file: &mut File, file_len: u64) -> Result<OcbMetadataV1> {
    let bootstrap = OcbBootstrapPageV1::read_from(&mut *file)?;
    bootstrap.root_ref.validate(OcbBodyKindV1::Root, file_len)?;
    if bootstrap.root_ref.length != OCB_ROOT_V1_LEN as u64 {
        return Err(ArcadiaTioError::ocb_corrupt_file(
            "OCB root object length is invalid",
        ));
    }

    let root_bytes = read_object_bytes(file, file_len, bootstrap.root_ref, OcbBodyKindV1::Root)?;
    let root = OcbRootV1::read_from(Cursor::new(root_bytes))?;
    read_metadata_objects(file, file_len, root)
}

fn read_metadata_v2(
    file: &mut File,
    file_len: u64,
    validation: OcbOpenValidationMode,
) -> Result<OcbMetadataV1> {
    let bootstrap = OcbBootstrapPageV2::read_from(&mut *file)?;
    select_v2_metadata(file, file_len, &bootstrap, validation)
}

pub(crate) fn analyze_v2_maintenance(
    path: &Path,
    validation: OcbOpenValidationMode,
) -> Result<OcbMaintenanceAnalysisV2> {
    let mut file = File::open(path)?;
    let file_len = file.metadata()?.len();
    if file_len < OCB_BOOTSTRAP_PAGE_V1_LEN as u64 {
        return Err(ArcadiaTioError::ocb_corrupt_file(
            "OCB file is shorter than bootstrap page",
        ));
    }
    let mut magic = [0u8; 8];
    file.seek(SeekFrom::Start(0))?;
    read_exact_ocb(&mut file, &mut magic)?;
    file.seek(SeekFrom::Start(0))?;
    match magic {
        OCB_BOOTSTRAP_MAGIC_V2 => {
            let bootstrap = OcbBootstrapPageV2::read_from(&mut file)?;
            analyze_v2_maintenance_from_bootstrap(&mut file, file_len, &bootstrap, validation)
        }
        OCB_BOOTSTRAP_MAGIC_V1 => Err(ArcadiaTioError::ocb_unsupported_format(
            "OCB maintenance analysis requires an appendable OCB file",
        )),
        _ => Err(ArcadiaTioError::ocb_unsupported_format(
            "invalid OCB bootstrap magic",
        )),
    }
}

fn analyze_v2_maintenance_from_bootstrap(
    file: &mut File,
    file_len: u64,
    bootstrap: &OcbBootstrapPageV2,
    validation: OcbOpenValidationMode,
) -> Result<OcbMaintenanceAnalysisV2> {
    let mut candidates = Vec::new();
    let mut diagnostics = Vec::new();
    for (idx, slot_result) in bootstrap.decoded_root_slots().into_iter().enumerate() {
        let slot_id = idx as u16;
        let slot = match slot_result {
            Ok(slot) => slot,
            Err(err) => {
                diagnostics.push(root_candidate_diagnostic(
                    Some(slot_id),
                    None,
                    format!("OCB root slot could not be decoded: {err}"),
                ));
                continue;
            }
        };
        if slot.is_empty() {
            continue;
        }
        if let Err(err) = slot.validate_candidate(slot_id, file_len) {
            diagnostics.push(root_candidate_diagnostic(
                Some(slot_id),
                Some(slot.generation),
                format!("OCB root slot candidate is invalid: {err}"),
            ));
            continue;
        }
        let root_bytes = match read_object_bytes(file, file_len, slot.root_ref, OcbBodyKindV1::Root)
        {
            Ok(bytes) => bytes,
            Err(err) => {
                diagnostics.push(root_candidate_diagnostic(
                    Some(slot_id),
                    Some(slot.generation),
                    format!("OCB root object could not be read: {err}"),
                ));
                continue;
            }
        };
        let root = match OcbRootV2::read_from(Cursor::new(root_bytes)) {
            Ok(root) => root,
            Err(err) => {
                diagnostics.push(root_candidate_diagnostic(
                    Some(slot_id),
                    Some(slot.generation),
                    format!("OCB root object could not be decoded: {err}"),
                ));
                continue;
            }
        };
        if let Err(err) = root.validate_references(file_len) {
            diagnostics.push(root_candidate_diagnostic(
                Some(slot_id),
                Some(slot.generation),
                format!("OCB root references are invalid: {err}"),
            ));
            continue;
        }
        if let Err(err) = slot.validate_root(&root) {
            diagnostics.push(root_candidate_diagnostic(
                Some(slot_id),
                Some(slot.generation),
                format!("OCB root slot/root metadata is inconsistent: {err}"),
            ));
            continue;
        }
        candidates.push(OcbRootCandidateV2 { slot, root });
    }

    while let Some(max_generation) = candidates
        .iter()
        .map(|candidate| candidate.slot.generation)
        .max()
    {
        let first_ref = candidates
            .iter()
            .find(|candidate| candidate.slot.generation == max_generation)
            .expect("max generation candidate exists")
            .slot
            .root_ref;
        let conflicting_same_generation = candidates.iter().any(|candidate| {
            candidate.slot.generation == max_generation && candidate.slot.root_ref != first_ref
        });
        if conflicting_same_generation {
            for candidate in candidates
                .iter()
                .filter(|candidate| candidate.slot.generation == max_generation)
            {
                diagnostics.push(root_candidate_diagnostic(
                    Some(candidate.slot.slot_id),
                    Some(candidate.slot.generation),
                    "OCB root generation has conflicting root references",
                ));
            }
            candidates.retain(|candidate| candidate.slot.generation != max_generation);
            continue;
        }
        let selected_idx = candidates
            .iter()
            .position(|candidate| candidate.slot.generation == max_generation)
            .expect("max generation candidate exists");
        let candidate = candidates.remove(selected_idx);
        match validate_v2_root_referenced_objects_with_scope(
            file,
            file_len,
            &candidate.root,
            validation,
        ) {
            Ok(metadata) => {
                let selected_root_end_offset = body_ref_end(candidate.slot.root_ref)?;
                let selected_snapshot_end_offset =
                    selected_snapshot_referenced_end(&candidate.slot, &candidate.root, &metadata)?;
                return Ok(OcbMaintenanceAnalysisV2 {
                    file_len,
                    selected_slot_id: candidate.slot.slot_id,
                    selected_root_generation: candidate.slot.generation,
                    previous_root_generation: (!candidate.slot.previous_root_ref.is_null())
                        .then_some(candidate.slot.previous_generation),
                    selected_root_end_offset,
                    selected_snapshot_end_offset,
                    metadata,
                    rejected_candidates: diagnostics,
                });
            }
            Err(err) => {
                diagnostics.push(root_candidate_diagnostic(
                    Some(candidate.slot.slot_id),
                    Some(candidate.slot.generation),
                    format!("OCB selected root candidate failed validation: {err}"),
                ));
                candidates.retain(|candidate| candidate.slot.generation != max_generation);
            }
        }
    }

    Err(ArcadiaTioError::ocb_corrupt_file(
        "OCB root selection found no valid root slot",
    ))
}

fn root_candidate_diagnostic(
    slot_id: Option<u16>,
    generation: Option<u64>,
    message: impl Into<String>,
) -> OcbRootCandidateDiagnosticV2 {
    OcbRootCandidateDiagnosticV2 {
        slot_id,
        generation,
        message: message.into(),
    }
}

fn select_v2_metadata(
    file: &mut File,
    file_len: u64,
    bootstrap: &OcbBootstrapPageV2,
    validation: OcbOpenValidationMode,
) -> Result<OcbMetadataV1> {
    let mut candidates = Vec::new();
    for (idx, slot_result) in bootstrap.decoded_root_slots().into_iter().enumerate() {
        let Ok(slot) = slot_result else {
            continue;
        };
        if slot.is_empty() || slot.validate_candidate(idx as u16, file_len).is_err() {
            continue;
        }
        let Ok(root_bytes) = read_object_bytes(file, file_len, slot.root_ref, OcbBodyKindV1::Root)
        else {
            continue;
        };
        let Ok(root) = OcbRootV2::read_from(Cursor::new(root_bytes)) else {
            continue;
        };
        if root.validate_references(file_len).is_err() || slot.validate_root(&root).is_err() {
            continue;
        }
        candidates.push(OcbRootCandidateV2 { slot, root });
    }
    choose_v2_metadata(file, file_len, candidates, validation)
}

fn choose_v2_metadata(
    file: &mut File,
    file_len: u64,
    mut candidates: Vec<OcbRootCandidateV2>,
    validation: OcbOpenValidationMode,
) -> Result<OcbMetadataV1> {
    while let Some(max_generation) = candidates
        .iter()
        .map(|candidate| candidate.slot.generation)
        .max()
    {
        let first_ref = candidates
            .iter()
            .find(|candidate| candidate.slot.generation == max_generation)
            .expect("max generation candidate exists")
            .slot
            .root_ref;
        let conflicting_same_generation = candidates.iter().any(|candidate| {
            candidate.slot.generation == max_generation && candidate.slot.root_ref != first_ref
        });
        if conflicting_same_generation {
            candidates.retain(|candidate| candidate.slot.generation != max_generation);
            continue;
        }
        let selected_idx = candidates
            .iter()
            .position(|candidate| candidate.slot.generation == max_generation)
            .expect("max generation candidate exists");
        let candidate = candidates.remove(selected_idx);
        if let Ok(metadata) = validate_v2_root_referenced_objects_with_scope(
            file,
            file_len,
            &candidate.root,
            validation,
        ) {
            return Ok(metadata);
        }
        candidates.retain(|candidate| candidate.slot.generation != max_generation);
    }

    Err(ArcadiaTioError::ocb_corrupt_file(
        "OCB root selection found no valid root slot",
    ))
}

pub(crate) fn read_metadata_objects(
    file: &mut File,
    file_len: u64,
    root: OcbRootV1,
) -> Result<OcbMetadataV1> {
    let schema_bytes = read_object_bytes(file, file_len, root.schema_ref, OcbBodyKindV1::Schema)?;
    let schema = OcbSchemaV1::read_from(Cursor::new(schema_bytes))?;

    let string_table_bytes = read_object_bytes(
        file,
        file_len,
        schema.string_table_ref,
        OcbBodyKindV1::StringTable,
    )?;
    let string_table = OcbStringTableV1::read_from(Cursor::new(string_table_bytes))?;

    let dictionary_index = if root.dictionary_index_ref.is_null() {
        None
    } else {
        let bytes = read_object_bytes(
            file,
            file_len,
            root.dictionary_index_ref,
            OcbBodyKindV1::DictionaryIndex,
        )?;
        Some(OcbDictionaryIndexV1::read_from(Cursor::new(bytes))?)
    };

    let row_group_index_bytes = read_object_bytes(
        file,
        file_len,
        root.row_group_index_ref,
        OcbBodyKindV1::RowGroupIndex,
    )?;
    let row_group_index = OcbRowGroupIndexV1::read_from(Cursor::new(row_group_index_bytes))?;

    let ordering_proof = if root.ordering_proof_ref.is_null() {
        None
    } else {
        let bytes = read_object_bytes(
            file,
            file_len,
            root.ordering_proof_ref,
            OcbBodyKindV1::OrderingProof,
        )?;
        Some(OcbOrderingProofV1::read_from(Cursor::new(bytes))?)
    };

    Ok(OcbMetadataV1 {
        root,
        string_table,
        schema,
        dictionary_index,
        row_group_index,
        ordering_proof,
        file_len,
        appendable: false,
        root_generation: 0,
        previous_root_generation: None,
    })
}

pub(crate) fn read_metadata_objects_v2(
    file: &mut File,
    file_len: u64,
    root: &OcbRootV2,
) -> Result<OcbMetadataV1> {
    let mut metadata = read_metadata_objects(file, file_len, root.to_v1_root())?;
    metadata.appendable = true;
    metadata.root_generation = root.generation;
    metadata.previous_root_generation =
        (!root.previous_root_ref.is_null()).then_some(root.previous_generation);
    if root.commit_diagnostics_ref.kind == OcbBodyKindV1::RowGroupIndexDelta {
        let delta_bytes = read_object_bytes(
            file,
            file_len,
            root.commit_diagnostics_ref,
            OcbBodyKindV1::RowGroupIndexDelta,
        )?;
        let delta = OcbRowGroupIndexDeltaV1::read_from(Cursor::new(delta_bytes))?;
        apply_row_group_index_delta(&mut metadata, delta)?;
    }
    Ok(metadata)
}

pub(crate) fn selected_snapshot_referenced_end(
    slot: &OcbRootSlotV2,
    root: &OcbRootV2,
    metadata: &OcbMetadataV1,
) -> Result<u64> {
    let mut max_end = OCB_BOOTSTRAP_PAGE_V2_LEN as u64;
    update_max_ref_end(&mut max_end, slot.root_ref)?;
    update_max_ref_end(&mut max_end, slot.previous_root_ref)?;
    update_max_ref_end(&mut max_end, slot.commit_diagnostics_ref)?;
    update_max_ref_end(&mut max_end, root.previous_root_ref)?;
    update_max_ref_end(&mut max_end, root.schema_ref)?;
    update_max_ref_end(&mut max_end, root.dictionary_index_ref)?;
    update_max_ref_end(&mut max_end, root.row_group_index_ref)?;
    update_max_ref_end(&mut max_end, root.ordering_proof_ref)?;
    update_max_ref_end(&mut max_end, root.debug_json_ref)?;
    update_max_ref_end(&mut max_end, root.first_key_tuple_ref)?;
    update_max_ref_end(&mut max_end, root.last_key_tuple_ref)?;
    update_max_ref_end(&mut max_end, root.append_first_key_tuple_ref)?;
    update_max_ref_end(&mut max_end, root.append_last_key_tuple_ref)?;
    update_max_ref_end(&mut max_end, root.commit_diagnostics_ref)?;
    update_max_ref_end(&mut max_end, metadata.schema.string_table_ref)?;
    if let Some(dictionary_index) = &metadata.dictionary_index {
        for dictionary in &dictionary_index.dictionaries {
            update_max_ref_end(&mut max_end, dictionary.values_ref)?;
        }
    }
    for row_group in &metadata.row_group_index.row_groups {
        update_max_ref_end(&mut max_end, row_group.first_key_tuple_ref)?;
        update_max_ref_end(&mut max_end, row_group.last_key_tuple_ref)?;
    }
    for chunk in &metadata.row_group_index.column_chunks {
        update_max_ref_end(&mut max_end, chunk.value_ref)?;
        update_max_ref_end(&mut max_end, chunk.validity_ref)?;
    }
    if let Some(ordering_proof) = &metadata.ordering_proof {
        for proof in &ordering_proof.row_group_proofs {
            update_max_ref_end(&mut max_end, proof.first_tuple_ref)?;
            update_max_ref_end(&mut max_end, proof.last_tuple_ref)?;
        }
    }
    Ok(max_end)
}

fn update_max_ref_end(max_end: &mut u64, reference: OcbBodyRefV2) -> Result<()> {
    if reference.is_null() {
        return Ok(());
    }
    let end = body_ref_end(reference)?;
    *max_end = (*max_end).max(end);
    Ok(())
}

fn body_ref_end(reference: OcbBodyRefV2) -> Result<u64> {
    reference
        .offset
        .checked_add(reference.length)
        .ok_or(ArcadiaTioError::ocb_invalid_input(
            "OCB body reference range overflows",
        ))
}

fn apply_row_group_index_delta(
    metadata: &mut OcbMetadataV1,
    delta: OcbRowGroupIndexDeltaV1,
) -> Result<()> {
    let index = &mut metadata.row_group_index;
    if index.row_groups.len() != delta.base_row_group_count as usize
        || index.column_chunks.len() != delta.base_column_chunk_count as usize
        || index.stats.len() != delta.base_stat_count as usize
    {
        return Err(ArcadiaTioError::ocb_corrupt_file(
            "OCB row-group index delta base counts do not match base index",
        ));
    }
    if delta.flags != index.flags {
        return Err(ArcadiaTioError::ocb_corrupt_file(
            "OCB row-group index delta flags do not match base index",
        ));
    }
    if delta.row_group_ordering_proofs.is_empty() {
        if delta.base_ordering_proof_count != 0 || !delta.ordering_keys.is_empty() {
            return Err(ArcadiaTioError::ocb_corrupt_file(
                "OCB row-group index delta ordering metadata is inconsistent",
            ));
        }
    } else {
        if delta.base_ordering_proof_count != delta.base_row_group_count
            || delta.row_group_ordering_proofs.len() != delta.row_groups.len()
        {
            return Err(ArcadiaTioError::ocb_corrupt_file(
                "OCB row-group index delta ordering partition does not match row-group delta",
            ));
        }
        let proof = metadata
            .ordering_proof
            .as_mut()
            .ok_or(ArcadiaTioError::ocb_corrupt_file(
                "OCB row-group index delta requires ordering proof",
            ))?;
        if proof.row_group_proofs.len() != delta.base_ordering_proof_count as usize {
            return Err(ArcadiaTioError::ocb_corrupt_file(
                "OCB row-group index delta ordering base count does not match base proof",
            ));
        }
        if proof.keys != delta.ordering_keys {
            return Err(ArcadiaTioError::ocb_corrupt_file(
                "OCB row-group index delta ordering keys do not match base proof",
            ));
        }
        proof
            .row_group_proofs
            .extend(delta.row_group_ordering_proofs);
    }
    index.row_groups.extend(delta.row_groups);
    index.column_chunks.extend(delta.column_chunks);
    index.stats.extend(delta.stats);
    Ok(())
}

pub(crate) fn read_column_chunk(
    path: &Path,
    file_len: u64,
    reference: OcbBodyRefV2,
) -> Result<OcbColumnChunkObjectV1> {
    let mut file = File::open(path)?;
    let bytes = read_object_bytes(&mut file, file_len, reference, OcbBodyKindV1::ColumnChunk)?;
    OcbColumnChunkObjectV1::read_from(Cursor::new(bytes))
}

pub(crate) fn validate_v2_root_referenced_objects(
    file: &mut File,
    file_len: u64,
    root: &OcbRootV2,
) -> Result<()> {
    validate_v2_root_referenced_objects_with_scope(
        file,
        file_len,
        root,
        OcbOpenValidationMode::FullPayload,
    )?;
    Ok(())
}

pub(crate) fn validate_v2_root_referenced_metadata(
    file: &mut File,
    file_len: u64,
    root: &OcbRootV2,
) -> Result<()> {
    validate_v2_root_referenced_objects_with_scope(
        file,
        file_len,
        root,
        OcbOpenValidationMode::MetadataGraph,
    )?;
    Ok(())
}

fn validate_v2_root_referenced_objects_with_scope(
    file: &mut File,
    file_len: u64,
    root: &OcbRootV2,
    validation: OcbOpenValidationMode,
) -> Result<OcbMetadataV1> {
    let metadata = read_metadata_objects_v2(file, file_len, root)?;
    validate_v2_metadata_graph(file, &metadata, root, validation)?;

    validate_optional_object(
        file,
        file_len,
        root.debug_json_ref,
        OcbBodyKindV1::DebugJsonMetadata,
    )?;
    validate_optional_object(
        file,
        file_len,
        root.first_key_tuple_ref,
        OcbBodyKindV1::KeyTuple,
    )?;
    validate_optional_object(
        file,
        file_len,
        root.last_key_tuple_ref,
        OcbBodyKindV1::KeyTuple,
    )?;
    validate_optional_object(
        file,
        file_len,
        root.append_first_key_tuple_ref,
        OcbBodyKindV1::KeyTuple,
    )?;
    validate_optional_object(
        file,
        file_len,
        root.append_last_key_tuple_ref,
        OcbBodyKindV1::KeyTuple,
    )?;
    match root.commit_diagnostics_ref.kind {
        OcbBodyKindV1::Unknown => {}
        OcbBodyKindV1::DebugJsonMetadata => validate_optional_object(
            file,
            file_len,
            root.commit_diagnostics_ref,
            OcbBodyKindV1::DebugJsonMetadata,
        )?,
        OcbBodyKindV1::RowGroupIndexDelta => validate_optional_object(
            file,
            file_len,
            root.commit_diagnostics_ref,
            OcbBodyKindV1::RowGroupIndexDelta,
        )?,
        _ => {
            return Err(ArcadiaTioError::ocb_corrupt_file(
                "OCB root commit_diagnostics_ref has invalid kind",
            ));
        }
    };
    Ok(metadata)
}

fn validate_v2_metadata_graph(
    file: &mut File,
    metadata: &OcbMetadataV1,
    root: &OcbRootV2,
    validation: OcbOpenValidationMode,
) -> Result<()> {
    validate_v2_root_semantics(root, metadata)?;
    validate_root_counts(metadata, root.column_chunk_count)?;
    let columns_by_id = validate_schema_graph(metadata)?;
    validate_dictionary_graph(file, metadata, &columns_by_id)?;
    validate_row_group_graph(file, metadata, &columns_by_id, validation)?;
    validate_ordering_graph(file, metadata, &columns_by_id)?;
    Ok(())
}

fn validate_v2_root_semantics(root: &OcbRootV2, metadata: &OcbMetadataV1) -> Result<()> {
    let is_first_root = root.previous_root_ref.is_null();
    if is_first_root {
        if root.previous_generation != 0 {
            return Err(ArcadiaTioError::ocb_corrupt_file(
                "OCB first root must not reference a previous generation",
            ));
        }
    } else if root.previous_generation.checked_add(1) != Some(root.generation) {
        return Err(ArcadiaTioError::ocb_corrupt_file(
            "OCB root previous generation metadata is invalid",
        ));
    }
    let append_row_end = root
        .append_base_row
        .checked_add(root.append_row_count)
        .ok_or(ArcadiaTioError::ocb_corrupt_file(
            "OCB root append row range overflows",
        ))?;
    if append_row_end > root.row_count {
        return Err(ArcadiaTioError::ocb_corrupt_file(
            "OCB root append row range exceeds total rows",
        ));
    }
    let append_group_end = root
        .append_base_row_group
        .checked_add(root.append_row_group_count)
        .ok_or(ArcadiaTioError::ocb_corrupt_file(
            "OCB root append row-group range overflows",
        ))?;
    if append_group_end > root.row_group_count {
        return Err(ArcadiaTioError::ocb_corrupt_file(
            "OCB root append row-group range exceeds total row groups",
        ));
    }
    if root.append_row_count == 0 || root.append_row_group_count == 0 {
        if !(is_first_root && root.row_count == 0 && root.row_group_count == 0) {
            return Err(ArcadiaTioError::ocb_corrupt_file(
                "OCB root append range must be non-empty",
            ));
        }
    }
    let appended_rows = metadata
        .row_group_index
        .row_groups
        .iter()
        .filter(|row_group| {
            row_group.row_group_id >= root.append_base_row_group
                && row_group.row_group_id < append_group_end
        })
        .try_fold(0u64, |acc, row_group| acc.checked_add(row_group.row_count))
        .ok_or(ArcadiaTioError::ocb_corrupt_file(
            "OCB root append row count overflows",
        ))?;
    if appended_rows != root.append_row_count {
        return Err(ArcadiaTioError::ocb_corrupt_file(
            "OCB root append row count does not match appended row groups",
        ));
    }
    Ok(())
}

fn validate_root_counts(metadata: &OcbMetadataV1, root_column_chunk_count: u32) -> Result<()> {
    if metadata.root.column_count as usize != metadata.schema.columns.len() {
        return Err(ArcadiaTioError::ocb_corrupt_file(
            "OCB root column_count does not match schema",
        ));
    }
    if metadata.root.row_group_count as usize != metadata.row_group_index.row_groups.len() {
        return Err(ArcadiaTioError::ocb_corrupt_file(
            "OCB root row_group_count does not match row-group index",
        ));
    }
    if root_column_chunk_count as usize != metadata.row_group_index.column_chunks.len() {
        return Err(ArcadiaTioError::ocb_corrupt_file(
            "OCB root column_chunk_count does not match row-group index",
        ));
    }
    if metadata.root.dictionary_count > 0 && metadata.dictionary_index.is_none() {
        return Err(ArcadiaTioError::ocb_corrupt_file(
            "OCB root dictionary_count requires dictionary index",
        ));
    }
    match (&metadata.dictionary_index, metadata.root.dictionary_count) {
        (Some(index), count) if index.dictionaries.len() != count as usize => {
            return Err(ArcadiaTioError::ocb_corrupt_file(
                "OCB root dictionary_count does not match dictionary index",
            ));
        }
        (None, 0) | (Some(_), _) => {}
        (None, _) => unreachable!("handled above"),
    }
    let total_rows = metadata
        .row_group_index
        .row_groups
        .iter()
        .try_fold(0u64, |acc, row_group| acc.checked_add(row_group.row_count))
        .ok_or(ArcadiaTioError::ocb_corrupt_file("OCB row_count overflows"))?;
    if total_rows != metadata.root.row_count {
        return Err(ArcadiaTioError::ocb_corrupt_file(
            "OCB root row_count does not match row groups",
        ));
    }
    Ok(())
}

fn validate_schema_graph<'a>(
    metadata: &'a OcbMetadataV1,
) -> Result<BTreeMap<u32, &'a super::format::OcbColumnDescV1>> {
    let mut columns_by_id = BTreeMap::new();
    let mut seen_names = BTreeSet::new();
    for column in &metadata.schema.columns {
        if columns_by_id.insert(column.column_id, column).is_some() {
            return Err(ArcadiaTioError::ocb_corrupt_file(
                "OCB schema has duplicate column ids",
            ));
        }
        let name = metadata
            .string_table
            .strings
            .get(column.name_string_id as usize)
            .ok_or(ArcadiaTioError::ocb_corrupt_file(
                "OCB column name string id is out of range",
            ))?;
        if !seen_names.insert(name) {
            return Err(ArcadiaTioError::ocb_corrupt_file(
                "OCB schema has duplicate column names",
            ));
        }
        column.value_byte_width()?;
        if column.logical_kind == OcbLogicalKindV1::DictionaryCode
            && column.dictionary_id == OCB_NULL_U32
        {
            return Err(ArcadiaTioError::ocb_corrupt_file(
                "OCB dictionary-coded column must reference a dictionary",
            ));
        }
        if column.logical_kind == OcbLogicalKindV1::DictionaryCode
            && column.physical_type == super::format::OcbPhysicalTypeV1::FixedBinary
        {
            return Err(ArcadiaTioError::ocb_corrupt_file(
                "OCB dictionary-coded column cannot use fixed-binary physical type",
            ));
        }
    }
    Ok(columns_by_id)
}

fn validate_dictionary_graph(
    file: &mut File,
    metadata: &OcbMetadataV1,
    columns_by_id: &BTreeMap<u32, &super::format::OcbColumnDescV1>,
) -> Result<()> {
    let Some(dictionary_index) = &metadata.dictionary_index else {
        for column in columns_by_id.values() {
            if column.logical_kind == OcbLogicalKindV1::DictionaryCode {
                return Err(ArcadiaTioError::ocb_corrupt_file(
                    "OCB dictionary-coded column references missing dictionary index",
                ));
            }
        }
        return Ok(());
    };

    let mut dictionaries_by_id = BTreeMap::new();
    let mut seen_names = BTreeSet::new();
    for dictionary in &dictionary_index.dictionaries {
        if dictionaries_by_id
            .insert(dictionary.dictionary_id, dictionary)
            .is_some()
        {
            return Err(ArcadiaTioError::ocb_corrupt_file(
                "OCB dictionary index has duplicate dictionary ids",
            ));
        }
        let name = metadata
            .string_table
            .strings
            .get(dictionary.name_string_id as usize)
            .ok_or(ArcadiaTioError::ocb_corrupt_file(
                "OCB dictionary name string id is out of range",
            ))?;
        if !seen_names.insert(name) {
            return Err(ArcadiaTioError::ocb_corrupt_file(
                "OCB dictionary index has duplicate dictionary names",
            ));
        }
        let values_bytes = read_object_bytes(
            file,
            metadata.file_len,
            dictionary.values_ref,
            OcbBodyKindV1::DictionaryValues,
        )?;
        let values = OcbDictionaryValuesV1::read_from(Cursor::new(values_bytes))?;
        if values.value_kind != dictionary.value_kind
            || values.values.len() != dictionary.entry_count as usize
        {
            return Err(ArcadiaTioError::ocb_corrupt_file(
                "OCB dictionary values do not match dictionary index",
            ));
        }
    }

    for column in columns_by_id.values() {
        if column.logical_kind != OcbLogicalKindV1::DictionaryCode {
            continue;
        }
        let dictionary = dictionaries_by_id.get(&column.dictionary_id).ok_or(
            ArcadiaTioError::ocb_corrupt_file("OCB column references unknown dictionary"),
        )?;
        if dictionary.code_physical_type != column.physical_type {
            return Err(ArcadiaTioError::ocb_corrupt_file(
                "OCB dictionary code physical type does not match column",
            ));
        }
    }
    Ok(())
}

fn validate_row_group_graph(
    file: &mut File,
    metadata: &OcbMetadataV1,
    columns_by_id: &BTreeMap<u32, &super::format::OcbColumnDescV1>,
    validation: OcbOpenValidationMode,
) -> Result<()> {
    let expected_columns = columns_by_id.keys().copied().collect::<BTreeSet<_>>();
    let mut seen_row_groups = BTreeSet::new();
    let mut expected_base_row = 0u64;
    let mut expected_chunk_begin = 0u64;
    let mut expected_stat_begin = 0u64;
    for row_group in &metadata.row_group_index.row_groups {
        validate_row_group_desc(
            row_group,
            &mut seen_row_groups,
            &mut expected_base_row,
            &mut expected_chunk_begin,
            &mut expected_stat_begin,
        )?;
        validate_optional_object(
            file,
            metadata.file_len,
            row_group.first_key_tuple_ref,
            OcbBodyKindV1::KeyTuple,
        )?;
        validate_optional_object(
            file,
            metadata.file_len,
            row_group.last_key_tuple_ref,
            OcbBodyKindV1::KeyTuple,
        )?;

        let chunks = checked_chunks_for_row_group(metadata, row_group)?;
        let mut chunk_columns = BTreeSet::new();
        for chunk in chunks {
            validate_chunk_desc(file, metadata, row_group, chunk, columns_by_id, validation)?;
            if !chunk_columns.insert(chunk.column_id) {
                return Err(ArcadiaTioError::ocb_corrupt_file(
                    "OCB row group has duplicate chunk descriptors for a column",
                ));
            }
        }
        if chunk_columns != expected_columns {
            return Err(ArcadiaTioError::ocb_corrupt_file(
                "OCB row group chunk descriptors do not cover all schema columns",
            ));
        }
        let stats = checked_stats_for_row_group(metadata, row_group)?;
        let mut stat_columns = BTreeSet::new();
        for stat in stats {
            validate_stat_desc(row_group, stat, columns_by_id)?;
            if !stat_columns.insert(stat.column_id) {
                return Err(ArcadiaTioError::ocb_corrupt_file(
                    "OCB row group has duplicate stats for a column",
                ));
            }
        }
    }
    if expected_chunk_begin != metadata.row_group_index.column_chunks.len() as u64 {
        return Err(ArcadiaTioError::ocb_corrupt_file(
            "OCB row-group chunk descriptor ranges do not cover the chunk table",
        ));
    }
    if expected_stat_begin != metadata.row_group_index.stats.len() as u64 {
        return Err(ArcadiaTioError::ocb_corrupt_file(
            "OCB row-group stat ranges do not cover the stats table",
        ));
    }
    Ok(())
}

fn validate_row_group_desc(
    row_group: &OcbRowGroupDescV1,
    seen_row_groups: &mut BTreeSet<u32>,
    expected_base_row: &mut u64,
    expected_chunk_begin: &mut u64,
    expected_stat_begin: &mut u64,
) -> Result<()> {
    if !seen_row_groups.insert(row_group.row_group_id) {
        return Err(ArcadiaTioError::ocb_corrupt_file(
            "OCB row-group index has duplicate row group ids",
        ));
    }
    if row_group.base_row != *expected_base_row {
        return Err(ArcadiaTioError::ocb_corrupt_file(
            "OCB row-group base rows are not contiguous",
        ));
    }
    if row_group.chunk_desc_begin != *expected_chunk_begin {
        return Err(ArcadiaTioError::ocb_corrupt_file(
            "OCB row-group chunk descriptor ranges are not contiguous",
        ));
    }
    if row_group.stat_begin != *expected_stat_begin {
        return Err(ArcadiaTioError::ocb_corrupt_file(
            "OCB row-group stat ranges are not contiguous",
        ));
    }
    *expected_base_row = expected_base_row.checked_add(row_group.row_count).ok_or(
        ArcadiaTioError::ocb_corrupt_file("OCB row-group base row range overflows"),
    )?;
    *expected_chunk_begin = expected_chunk_begin
        .checked_add(u64::from(row_group.chunk_desc_count))
        .ok_or(ArcadiaTioError::ocb_corrupt_file(
            "OCB row-group chunk descriptor range overflows",
        ))?;
    *expected_stat_begin = expected_stat_begin
        .checked_add(u64::from(row_group.stat_count))
        .ok_or(ArcadiaTioError::ocb_corrupt_file(
            "OCB row-group stat descriptor range overflows",
        ))?;
    Ok(())
}

fn validate_chunk_desc(
    file: &mut File,
    metadata: &OcbMetadataV1,
    row_group: &OcbRowGroupDescV1,
    chunk: &OcbColumnChunkDescV1,
    columns_by_id: &BTreeMap<u32, &super::format::OcbColumnDescV1>,
    validation: OcbOpenValidationMode,
) -> Result<()> {
    if chunk.row_group_id != row_group.row_group_id {
        return Err(ArcadiaTioError::ocb_corrupt_file(
            "OCB row-group chunk descriptor references a different row group",
        ));
    }
    let column = columns_by_id
        .get(&chunk.column_id)
        .ok_or(ArcadiaTioError::ocb_corrupt_file(
            "OCB chunk descriptor references unknown column",
        ))?;
    if chunk.physical_type != column.physical_type {
        return Err(ArcadiaTioError::ocb_corrupt_file(
            "OCB chunk descriptor does not match schema",
        ));
    }
    if chunk.row_count != row_group.row_count {
        return Err(ArcadiaTioError::ocb_corrupt_file(
            "OCB chunk row_count does not match row group",
        ));
    }
    let expected_bytes = column.expected_value_bytes(chunk.row_count)?;
    if chunk.uncompressed_bytes != expected_bytes {
        return Err(ArcadiaTioError::ocb_corrupt_file(
            "OCB chunk byte length does not match row count and physical type",
        ));
    }
    match validation {
        OcbOpenValidationMode::MetadataGraph => {
            validate_column_chunk_object_header(file, metadata.file_len, chunk)?;
        }
        OcbOpenValidationMode::FullPayload => {
            validate_column_chunk_object_streaming(file, metadata.file_len, chunk)?;
        }
    }
    if !chunk.validity_ref.is_null() {
        if column.nullability != OcbNullabilityV1::Nullable {
            return Err(ArcadiaTioError::ocb_corrupt_file(
                "OCB non-null column chunk cannot have a validity bitmap",
            ));
        }
        match validation {
            OcbOpenValidationMode::MetadataGraph => {
                chunk
                    .validity_ref
                    .validate(OcbBodyKindV1::ValidityBitmap, metadata.file_len)?;
                if chunk.validity_ref.length != chunk.row_count.div_ceil(8) {
                    return Err(ArcadiaTioError::ocb_corrupt_file(
                        "OCB validity bitmap length does not match row count",
                    ));
                }
            }
            OcbOpenValidationMode::FullPayload => {
                let validity = read_object_bytes(
                    file,
                    metadata.file_len,
                    chunk.validity_ref,
                    OcbBodyKindV1::ValidityBitmap,
                )?;
                if validity.len() as u64 != chunk.row_count.div_ceil(8) {
                    return Err(ArcadiaTioError::ocb_corrupt_file(
                        "OCB validity bitmap length does not match row count",
                    ));
                }
            }
        }
    }
    Ok(())
}

fn validate_stat_desc(
    row_group: &OcbRowGroupDescV1,
    stat: &OcbColumnStatsV1,
    columns_by_id: &BTreeMap<u32, &super::format::OcbColumnDescV1>,
) -> Result<()> {
    if stat.row_group_id != row_group.row_group_id {
        return Err(ArcadiaTioError::ocb_corrupt_file(
            "OCB row-group stat references a different row group",
        ));
    }
    let column = columns_by_id
        .get(&stat.column_id)
        .ok_or(ArcadiaTioError::ocb_corrupt_file(
            "OCB stats descriptor references unknown column",
        ))?;
    if stat.physical_type != column.physical_type {
        return Err(ArcadiaTioError::ocb_corrupt_file(
            "OCB stat dtype does not match schema column dtype",
        ));
    }
    if stat.physical_type == super::format::OcbPhysicalTypeV1::FixedBinary {
        return Err(ArcadiaTioError::ocb_corrupt_file(
            "OCB fixed-binary columns do not support scalar stats",
        ));
    }
    Ok(())
}

fn validate_ordering_graph(
    file: &mut File,
    metadata: &OcbMetadataV1,
    columns_by_id: &BTreeMap<u32, &super::format::OcbColumnDescV1>,
) -> Result<()> {
    let Some(ordering_proof) = &metadata.ordering_proof else {
        return Ok(());
    };
    if ordering_proof.row_group_proofs.len() != metadata.row_group_index.row_groups.len() {
        return Err(ArcadiaTioError::ocb_corrupt_file(
            "OCB ordering proof row-group count does not match row-group index",
        ));
    }
    let mut seen_key_columns = BTreeSet::new();
    for key in &ordering_proof.keys {
        let column = columns_by_id
            .get(&key.column_id)
            .ok_or(ArcadiaTioError::ocb_corrupt_file(
                "OCB ordering proof key references unknown column",
            ))?;
        if column.physical_type == super::format::OcbPhysicalTypeV1::FixedBinary {
            return Err(ArcadiaTioError::ocb_corrupt_file(
                "OCB fixed-binary column cannot be an ordering key",
            ));
        }
        if !seen_key_columns.insert(key.column_id) {
            return Err(ArcadiaTioError::ocb_corrupt_file(
                "OCB ordering proof has duplicate key columns",
            ));
        }
    }
    let row_groups_by_id = metadata
        .row_group_index
        .row_groups
        .iter()
        .map(|row_group| (row_group.row_group_id, row_group))
        .collect::<BTreeMap<_, _>>();
    let mut seen_proof_row_groups = BTreeSet::new();
    for proof in &ordering_proof.row_group_proofs {
        if !row_groups_by_id.contains_key(&proof.row_group_id) {
            return Err(ArcadiaTioError::ocb_corrupt_file(
                "OCB ordering proof references unknown row group",
            ));
        }
        if !seen_proof_row_groups.insert(proof.row_group_id) {
            return Err(ArcadiaTioError::ocb_corrupt_file(
                "OCB ordering proof has duplicate row-group proofs",
            ));
        }
        validate_optional_object(
            file,
            metadata.file_len,
            proof.first_tuple_ref,
            OcbBodyKindV1::KeyTuple,
        )?;
        validate_optional_object(
            file,
            metadata.file_len,
            proof.last_tuple_ref,
            OcbBodyKindV1::KeyTuple,
        )?;
    }
    if seen_proof_row_groups != row_groups_by_id.keys().copied().collect::<BTreeSet<_>>() {
        return Err(ArcadiaTioError::ocb_corrupt_file(
            "OCB ordering proof row groups do not cover row-group index",
        ));
    }
    Ok(())
}

fn checked_chunks_for_row_group<'a>(
    metadata: &'a OcbMetadataV1,
    row_group: &OcbRowGroupDescV1,
) -> Result<&'a [OcbColumnChunkDescV1]> {
    let begin = usize::try_from(row_group.chunk_desc_begin).map_err(|_| {
        ArcadiaTioError::ocb_corrupt_file("OCB row-group chunk descriptor begin is too large")
    })?;
    let end = begin
        .checked_add(row_group.chunk_desc_count as usize)
        .ok_or(ArcadiaTioError::ocb_corrupt_file(
            "OCB row-group chunk descriptor range overflows",
        ))?;
    metadata
        .row_group_index
        .column_chunks
        .get(begin..end)
        .ok_or(ArcadiaTioError::ocb_corrupt_file(
            "OCB row-group chunk descriptor range is out of bounds",
        ))
}

fn checked_stats_for_row_group<'a>(
    metadata: &'a OcbMetadataV1,
    row_group: &OcbRowGroupDescV1,
) -> Result<&'a [OcbColumnStatsV1]> {
    let begin = usize::try_from(row_group.stat_begin).map_err(|_| {
        ArcadiaTioError::ocb_corrupt_file("OCB row-group stat descriptor begin is too large")
    })?;
    let end = begin.checked_add(row_group.stat_count as usize).ok_or(
        ArcadiaTioError::ocb_corrupt_file("OCB row-group stat descriptor range overflows"),
    )?;
    metadata
        .row_group_index
        .stats
        .get(begin..end)
        .ok_or(ArcadiaTioError::ocb_corrupt_file(
            "OCB row-group stat descriptor range is out of bounds",
        ))
}

fn validate_column_chunk_object_header(
    file: &mut File,
    file_len: u64,
    chunk: &OcbColumnChunkDescV1,
) -> Result<[u8; OCB_COLUMN_CHUNK_V1_HEADER_LEN as usize]> {
    let reference = chunk.value_ref;
    reference.validate(OcbBodyKindV1::ColumnChunk, file_len)?;
    if reference.length < u64::from(OCB_COLUMN_CHUNK_V1_HEADER_LEN) + 4 {
        return Err(ArcadiaTioError::ocb_corrupt_file(
            "OCB column chunk object is too short",
        ));
    }

    file.seek(SeekFrom::Start(reference.offset))?;
    let mut header = [0u8; OCB_COLUMN_CHUNK_V1_HEADER_LEN as usize];
    read_exact_ocb(file, &mut header)?;
    if header[0..8] != OCB_COLUMN_CHUNK_MAGIC_V1 {
        return Err(ArcadiaTioError::ocb_corrupt_file(
            "invalid OCB column chunk magic",
        ));
    }
    let version = u16::from_le_bytes(header[8..10].try_into().expect("version bytes"));
    let header_len = u16::from_le_bytes(header[10..12].try_into().expect("header len bytes"));
    let physical_type_raw = u16::from_le_bytes(header[12..14].try_into().expect("dtype bytes"));
    let codec_raw = u16::from_le_bytes(header[14..16].try_into().expect("codec bytes"));
    let row_group_id = u32::from_le_bytes(header[20..24].try_into().expect("row group bytes"));
    let column_id = u32::from_le_bytes(header[24..28].try_into().expect("column bytes"));
    let row_count = u64::from_le_bytes(header[28..36].try_into().expect("row count bytes"));
    let value_bytes = u64::from_le_bytes(header[36..44].try_into().expect("value bytes"));
    if version != 1
        || header_len != OCB_COLUMN_CHUNK_V1_HEADER_LEN
        || physical_type_raw != chunk.physical_type as u16
        || codec_raw != chunk.codec as u16
        || row_group_id != chunk.row_group_id
        || column_id != chunk.column_id
        || row_count != chunk.row_count
        || value_bytes != chunk.uncompressed_bytes
    {
        return Err(ArcadiaTioError::ocb_corrupt_file(
            "OCB column chunk object does not match descriptor",
        ));
    }
    let min_object_len = u64::from(OCB_COLUMN_CHUNK_V1_HEADER_LEN) + 4;
    if reference.length < min_object_len {
        return Err(ArcadiaTioError::ocb_corrupt_file(
            "OCB column chunk value byte length does not match descriptor",
        ));
    }
    if chunk.codec == OcbChunkCodecV1::None
        && u64::from(OCB_COLUMN_CHUNK_V1_HEADER_LEN) + value_bytes + 4 != reference.length
    {
        return Err(ArcadiaTioError::ocb_corrupt_file(
            "OCB uncompressed column chunk object length does not match descriptor",
        ));
    }
    if chunk.codec == OcbChunkCodecV1::Zstd && reference.length == min_object_len {
        return Err(ArcadiaTioError::ocb_corrupt_file(
            "OCB zstd column chunk object has empty payload",
        ));
    }
    Ok(header)
}

fn validate_column_chunk_object_streaming(
    file: &mut File,
    file_len: u64,
    chunk: &OcbColumnChunkDescV1,
) -> Result<()> {
    let reference = chunk.value_ref;
    let header = validate_column_chunk_object_header(file, file_len, chunk)?;
    let encoded_bytes = reference
        .length
        .checked_sub(u64::from(OCB_COLUMN_CHUNK_V1_HEADER_LEN) + 4)
        .ok_or(ArcadiaTioError::ocb_corrupt_file(
            "OCB column chunk object is too short",
        ))?;

    let mut body_crc = crc32c_update(crc32c_init(), &header);
    let mut object_crc = crc32c_update(crc32c_init(), &header);
    let mut remaining = encoded_bytes;
    let mut buf = [0u8; 8192];
    if chunk.codec == OcbChunkCodecV1::Zstd && encoded_bytes > usize::MAX as u64 {
        return Err(ArcadiaTioError::ocb_corrupt_file(
            "OCB zstd column chunk payload exceeds addressable memory",
        ));
    }
    let mut compressed_payload = if chunk.codec == OcbChunkCodecV1::Zstd {
        Some(Vec::with_capacity(encoded_bytes as usize))
    } else {
        None
    };
    while remaining > 0 {
        let take = buf.len().min(remaining as usize);
        read_exact_ocb(file, &mut buf[..take])?;
        body_crc = crc32c_update(body_crc, &buf[..take]);
        object_crc = crc32c_update(object_crc, &buf[..take]);
        if let Some(payload) = compressed_payload.as_mut() {
            payload.extend_from_slice(&buf[..take]);
        }
        remaining -= take as u64;
    }
    let mut trailing_crc = [0u8; 4];
    read_exact_ocb(file, &mut trailing_crc)?;
    body_crc = crc32c_update(body_crc, &trailing_crc);
    object_crc = crc32c_update(object_crc, &[0, 0, 0, 0]);
    if reference.checksum_kind == OcbChecksumKindV1::Crc32c
        && crc32c_finish(body_crc) != reference.checksum
    {
        return Err(ArcadiaTioError::ocb_corrupt_file(
            "OCB body reference checksum mismatch",
        ));
    }
    if crc32c_finish(object_crc) != u32::from_le_bytes(trailing_crc) {
        return Err(ArcadiaTioError::ocb_corrupt_file(
            "OCB column chunk crc mismatch",
        ));
    }
    if let Some(payload) = compressed_payload {
        let object = OcbColumnChunkObjectV1 {
            version: 1,
            physical_type: chunk.physical_type,
            codec: chunk.codec,
            flags: 0,
            row_group_id: chunk.row_group_id,
            column_id: chunk.column_id,
            row_count: chunk.row_count,
            uncompressed_bytes: chunk.uncompressed_bytes,
            payload,
            crc32c: u32::from_le_bytes(trailing_crc),
        };
        let decoded = object.decode_payload()?;
        if decoded.len() as u64 != chunk.uncompressed_bytes {
            return Err(ArcadiaTioError::ocb_corrupt_file(
                "OCB zstd column chunk decoded byte length does not match descriptor",
            ));
        }
    }
    Ok(())
}

fn validate_optional_object(
    file: &mut File,
    file_len: u64,
    reference: OcbBodyRefV2,
    kind: OcbBodyKindV1,
) -> Result<()> {
    if !reference.is_null() {
        let _bytes = read_object_bytes(file, file_len, reference, kind)?;
    }
    Ok(())
}

pub(crate) fn read_object_bytes(
    file: &mut File,
    file_len: u64,
    reference: OcbBodyRefV2,
    expected_kind: OcbBodyKindV1,
) -> Result<Vec<u8>> {
    read_object_bytes_inner(file, file_len, reference, expected_kind, None)
}

pub(crate) fn read_object_bytes_with_attribution(
    file: &mut File,
    file_len: u64,
    reference: OcbBodyRefV2,
    expected_kind: OcbBodyKindV1,
    attribution: &mut OcbReadObjectAttribution,
) -> Result<Vec<u8>> {
    read_object_bytes_inner(file, file_len, reference, expected_kind, Some(attribution))
}

fn read_object_bytes_inner(
    file: &mut File,
    file_len: u64,
    reference: OcbBodyRefV2,
    expected_kind: OcbBodyKindV1,
    mut attribution: Option<&mut OcbReadObjectAttribution>,
) -> Result<Vec<u8>> {
    reference.validate(expected_kind, file_len)?;
    if reference.length > usize::MAX as u64 {
        return Err(ArcadiaTioError::ocb_corrupt_file(
            "OCB body reference length exceeds addressable memory",
        ));
    }
    let read_started = Instant::now();
    file.seek(SeekFrom::Start(reference.offset))?;
    let mut bytes = vec![0u8; reference.length as usize];
    read_exact_ocb(file, &mut bytes)?;
    if let Some(attr) = attribution.as_mut() {
        attr.read_io += read_started.elapsed();
        attr.bytes_read = attr.bytes_read.saturating_add(reference.length);
    }
    match reference.checksum_kind {
        OcbChecksumKindV1::None => {}
        OcbChecksumKindV1::Crc32c => {
            let checksum_started = Instant::now();
            let actual = crc32c(&bytes);
            if let Some(attr) = attribution.as_mut() {
                attr.checksum += checksum_started.elapsed();
            }
            if actual != reference.checksum {
                return Err(ArcadiaTioError::ocb_corrupt_file(
                    "OCB body reference checksum mismatch",
                ));
            }
        }
    }
    Ok(bytes)
}
