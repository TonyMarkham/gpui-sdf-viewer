use crate::{OfflineError, OfflineResult, PazEntry};

// ---------------------------------------------------------------------------------------------- //

use soul_attributes::soul;
use std::{collections::HashMap, path::Path};

// ---------------------------------------------------------------------------------------------- //

const MAGIC_SIZE: usize = 4;
const U8_SIZE: usize = 1;
const U32_SIZE: usize = 4;
const PAZ_TABLE_HEADER_SIZE: usize = 8;
const PAZ_TABLE_ENTRY_SIZE: usize = 8;
const PAZ_TABLE_SEPARATOR_SIZE: usize = 4;
const FOLDER_HASH_SIZE: usize = 4;
const FOLDER_RECORD_SIZE: usize = 16;
const RECORD_SIZE: usize = 20;
const MAX_PATH_DEPTH: usize = 64;
const PARENT_SENTINEL: u32 = u32::MAX;
const PAZ_INDEX_MASK: u32 = 0xFF;

#[derive(Debug)]
pub(crate) struct Pamt {
    pub(crate) entries: Vec<PazEntry>,
}

impl Pamt {
    #[soul(id = "concept.game-data-pipeline", step = "pamt table parse")]
    pub(crate) fn parse(data: &[u8], paz_dir: &Path, paz_stem: u32) -> OfflineResult<Pamt> {
        let mut off = 0usize;
        off += MAGIC_SIZE;

        let paz_count = read_u32(data, &mut off)? as usize;
        off += PAZ_TABLE_HEADER_SIZE;
        off += paz_count * PAZ_TABLE_ENTRY_SIZE
            + paz_count.saturating_sub(1) * PAZ_TABLE_SEPARATOR_SIZE;

        let folder_size = read_u32(data, &mut off)? as usize;
        let folder_end = (off + folder_size).min(data.len());
        let mut folder_prefix = String::new();
        while off < folder_end {
            let parent = read_u32(data, &mut off)?;
            let name = read_string(data, &mut off)?;
            if parent == PARENT_SENTINEL {
                folder_prefix = name;
            }
        }

        let node_size = read_u32(data, &mut off)? as usize;
        let node_start = off;
        let node_end = (node_start + node_size).min(data.len());
        let mut nodes: HashMap<u32, (u32, String)> = HashMap::new();
        while off < node_end {
            let rel = (off - node_start) as u32;
            let parent = read_u32(data, &mut off)?;
            let name = read_string(data, &mut off)?;
            nodes.insert(rel, (parent, name));
        }

        let folder_count = read_u32(data, &mut off)? as usize;
        off += FOLDER_HASH_SIZE;
        off += folder_count * FOLDER_RECORD_SIZE;

        let mut entries = Vec::new();
        while off + RECORD_SIZE <= data.len() {
            let node_ref = read_u32(data, &mut off)?;
            let offset = read_u32(data, &mut off)?;
            let comp_size = read_u32(data, &mut off)?;
            let orig_size = read_u32(data, &mut off)?;
            let flags = read_u32(data, &mut off)?;

            let paz_index = flags & PAZ_INDEX_MASK;
            let paz_num = paz_stem.checked_add(paz_index).ok_or_else(|| {
                OfflineError::paz(format!("paz index overflow: {paz_stem} + {paz_index}"))
            })?;
            let node_path = build_path(&nodes, node_ref);
            let path = if folder_prefix.is_empty() {
                node_path
            } else {
                format!("{folder_prefix}/{node_path}")
            };

            entries.push(PazEntry {
                path,
                paz_file: paz_dir.join(format!("{paz_num}.paz")),
                offset: offset as u64,
                comp_size,
                orig_size,
                flags,
            });
        }

        Ok(Pamt { entries })
    }
}

// ---------------------------------------------------------------------------------------------- //

fn read_u32(data: &[u8], off: &mut usize) -> OfflineResult<u32> {
    let start = *off;
    let bytes = data
        .get(start..start + U32_SIZE)
        .ok_or_else(|| OfflineError::paz(format!("pamt truncated at offset {start}")))?;
    *off = start + U32_SIZE;
    Ok(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
}

fn read_u8(data: &[u8], off: &mut usize) -> OfflineResult<u8> {
    let start = *off;
    let byte = *data
        .get(start)
        .ok_or_else(|| OfflineError::paz(format!("pamt truncated at offset {start}")))?;
    *off = start + U8_SIZE;
    Ok(byte)
}

fn read_string(data: &[u8], off: &mut usize) -> OfflineResult<String> {
    let start = *off;
    let len = read_u8(data, off)? as usize;
    let bytes = data
        .get(*off..*off + len)
        .ok_or_else(|| OfflineError::paz(format!("pamt truncated at offset {start}")))?;
    *off += len;
    Ok(String::from_utf8_lossy(bytes).into_owned())
}

fn build_path(nodes: &HashMap<u32, (u32, String)>, start: u32) -> String {
    let mut cur = start;
    let mut parts: Vec<&str> = Vec::new();
    let mut depth = 0;

    while cur != PARENT_SENTINEL && depth < MAX_PATH_DEPTH {
        match nodes.get(&cur) {
            Some((parent, name)) => {
                parts.push(name.as_str());
                cur = *parent;
            }
            None => break,
        }
        depth += 1;
    }

    parts.iter().rev().copied().collect()
}
