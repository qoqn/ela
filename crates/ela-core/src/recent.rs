use std::fs;
use std::io;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::StoreError;
use crate::storage::{Versioned, current_version, decode_with, write_atomically};

pub const RECENT_LIMIT: usize = 8;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
pub struct RecentEntry {
    pub id: Uuid,
    pub opened: i64,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct RecentFile {
    #[serde(default)]
    opened: Vec<RecentEntry>,
}

/// Kept apart from the configuration: it changes on every connect, and it is this machine's
/// history, so a synced config folder would otherwise conflict.
#[derive(Debug, Clone)]
pub struct Recent {
    path: PathBuf,
}

impl Recent {
    pub fn open_default() -> Option<Self> {
        Some(Self::at(dirs::data_dir()?.join("ela").join("recent.toml")))
    }

    pub fn at(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    pub fn entries(&self) -> Result<Vec<RecentEntry>, StoreError> {
        let text = match fs::read_to_string(&self.path) {
            Ok(text) => text,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(source) => {
                return Err(StoreError::Io {
                    path: self.path.clone(),
                    source,
                });
            }
        };
        let file: RecentFile = decode_with(&text, &[]).map_err(|error| StoreError::Io {
            path: self.path.clone(),
            source: io::Error::other(error.to_string()),
        })?;
        Ok(file.opened)
    }

    // `keep` lets deleted Connections fall off rather than hold places in the list.
    pub fn opened(&self, id: Uuid, at: i64, keep: impl Fn(Uuid) -> bool) -> Result<(), StoreError> {
        // A file that can't be read is started afresh: it is only history.
        let older = self.entries().unwrap_or_default();
        let mut opened = vec![RecentEntry { id, opened: at }];
        opened.extend(
            older
                .into_iter()
                .filter(|entry| entry.id != id && keep(entry.id)),
        );
        opened.truncate(RECENT_LIMIT);
        self.write(opened)
    }

    pub fn forget(&self, id: Uuid) -> Result<(), StoreError> {
        let mut opened = self.entries()?;
        opened.retain(|entry| entry.id != id);
        self.write(opened)
    }

    pub fn clear(&self) -> Result<(), StoreError> {
        self.write(Vec::new())
    }

    fn write(&self, opened: Vec<RecentEntry>) -> Result<(), StoreError> {
        let text = toml::to_string(&Versioned {
            schema_version: current_version(&[]),
            item: &RecentFile { opened },
        })?;
        write_atomically(&self.path, &text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn recent() -> (tempfile::TempDir, Recent) {
        let dir = tempfile::tempdir().unwrap();
        let recent = Recent::at(dir.path().join("share/ela/recent.toml"));
        (dir, recent)
    }

    fn ids(recent: &Recent) -> Vec<Uuid> {
        recent.entries().unwrap().iter().map(|e| e.id).collect()
    }

    #[test]
    fn nothing_opened_yet_is_an_empty_list() {
        let (_dir, recent) = recent();
        assert!(recent.entries().unwrap().is_empty());
    }

    #[test]
    fn the_last_opened_comes_first_once_and_the_list_stops_at_eight() {
        let (_dir, recent) = recent();
        let all: Vec<Uuid> = (0..10).map(|_| Uuid::new_v4()).collect();
        for (at, id) in all.iter().enumerate() {
            recent.opened(*id, at as i64, |_| true).unwrap();
        }
        recent.opened(all[5], 100, |_| true).unwrap();

        let listed = ids(&recent);
        assert_eq!(listed.len(), RECENT_LIMIT);
        assert_eq!(listed[0], all[5]);
        assert_eq!(listed[1], all[9]);
        assert_eq!(listed.iter().filter(|id| **id == all[5]).count(), 1);
        assert!(!listed.contains(&all[0]));
        assert_eq!(recent.entries().unwrap()[0].opened, 100);
    }

    #[test]
    fn the_list_survives_reopening_and_is_versioned() {
        let (_dir, recent) = recent();
        let id = Uuid::new_v4();
        recent.opened(id, 42, |_| true).unwrap();
        let again = Recent::at(recent.path.clone());
        assert_eq!(again.entries().unwrap(), [RecentEntry { id, opened: 42 }]);
        let text = fs::read_to_string(&recent.path).unwrap();
        assert!(text.starts_with("schema_version = 1\n"), "{text}");
    }

    #[test]
    fn entries_that_name_nothing_fall_off_on_the_next_open() {
        let (_dir, recent) = recent();
        let (gone, kept, new) = (Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4());
        recent.opened(gone, 1, |_| true).unwrap();
        recent.opened(kept, 2, |_| true).unwrap();
        recent.opened(new, 3, |id| id != gone).unwrap();
        assert_eq!(ids(&recent), [new, kept]);
    }

    #[test]
    fn forgetting_and_clearing() {
        let (_dir, recent) = recent();
        let (a, b) = (Uuid::new_v4(), Uuid::new_v4());
        recent.opened(a, 1, |_| true).unwrap();
        recent.opened(b, 2, |_| true).unwrap();
        recent.forget(b).unwrap();
        assert_eq!(ids(&recent), [a]);
        recent.clear().unwrap();
        assert!(ids(&recent).is_empty());
    }

    #[test]
    fn a_damaged_file_is_reported_but_opening_starts_it_afresh() {
        let (_dir, recent) = recent();
        fs::create_dir_all(recent.path.parent().unwrap()).unwrap();
        fs::write(&recent.path, "not toml [").unwrap();
        assert!(recent.entries().is_err());
        let id = Uuid::new_v4();
        recent.opened(id, 1, |_| true).unwrap();
        assert_eq!(ids(&recent), [id]);
    }
}
