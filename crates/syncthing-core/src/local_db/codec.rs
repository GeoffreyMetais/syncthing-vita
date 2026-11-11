//! On-disk text format of a [`FolderDb`] (`#vita-db-v1`, one tab-separated line per entry).

use std::collections::HashMap;

use super::{FolderDb, LocalEntry};
use crate::bep::{BepBlockInfo, BepVersionCounter};

impl FolderDb {
    /// Serializes the database to its persistent text form.
    pub fn serialize(&self) -> String {
        let mut out = format!("#vita-db-v1\t{:x}\t{}\n", self.index_id, self.max_sequence);
        for e in self.sorted_entries() {
            let ver: Vec<String> = e
                .version
                .iter()
                .map(|c| format!("{:x}:{}", c.short_device_id, c.value))
                .collect();
            let blocks: Vec<String> = e
                .blocks
                .iter()
                .map(|b| format!("{}:{}:{}", b.offset, b.size, to_hex(&b.hash)))
                .collect();
            out.push_str(&format!(
                "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\n",
                if e.is_dir { "D" } else { "F" },
                u8::from(e.deleted),
                e.size,
                e.modified_s,
                e.disk_mtime,
                e.sequence,
                ver.join(","),
                blocks.join(","),
                e.name
            ));
        }
        out
    }

    /// Parses the persistent text form; `None` if the header is missing or invalid.
    pub fn parse(text: &str) -> Option<Self> {
        let mut lines = text.lines();
        let header: Vec<&str> = lines.next()?.split('\t').collect();
        if header.len() != 3 || header[0] != "#vita-db-v1" {
            return None;
        }
        let mut db = Self {
            index_id: u64::from_str_radix(header[1], 16).ok()?,
            max_sequence: header[2].parse().ok()?,
            entries: HashMap::new(),
            dirty: false,
        };
        for line in lines {
            let p: Vec<&str> = line.splitn(9, '\t').collect();
            if p.len() != 9 || p[8].is_empty() {
                continue;
            }
            let version = p[6]
                .split(',')
                .filter(|s| !s.is_empty())
                .filter_map(|c| {
                    let (id, val) = c.split_once(':')?;
                    Some(BepVersionCounter {
                        short_device_id: u64::from_str_radix(id, 16).ok()?,
                        value: val.parse().ok()?,
                    })
                })
                .collect();
            let blocks = p[7]
                .split(',')
                .filter(|s| !s.is_empty())
                .filter_map(|b| {
                    let mut it = b.splitn(3, ':');
                    Some(BepBlockInfo {
                        offset: it.next()?.parse().ok()?,
                        size: it.next()?.parse().ok()?,
                        hash: from_hex(it.next()?)?,
                    })
                })
                .collect();
            let entry = LocalEntry {
                name: p[8].to_owned(),
                is_dir: p[0] == "D",
                deleted: p[1] == "1",
                size: p[2].parse().unwrap_or(0),
                modified_s: p[3].parse().unwrap_or(0),
                disk_mtime: p[4].parse().unwrap_or(0),
                sequence: p[5].parse().unwrap_or(0),
                version,
                blocks,
            };
            db.max_sequence = db.max_sequence.max(entry.sequence);
            db.entries.insert(entry.name.clone(), entry);
        }
        Some(db)
    }
}

fn to_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{:02x}", b)).collect()
}

fn from_hex(s: &str) -> Option<Vec<u8>> {
    if s.len() % 2 != 0 {
        return None;
    }
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).ok())
        .collect()
}
