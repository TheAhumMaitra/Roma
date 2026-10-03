// SPDX-FileCopyrightText: 2026 Ahum Maitra <theahummaitra@gmail.com>
// SPDX-License-Identifier: GPL-3.0-or-later

//    Copyright (C) 2026 Ahum Maitra

//      This program is free software: you can redistribute it and/or modify
//      it under the terms of the GNU General Public License as published by
//      the Free Software Foundation, either version 3 of the License, or
//      (at your option) any later version.

//      This program is distributed in the hope that it will be useful,
//      but WITHOUT ANY WARRANTY; without even the implied warranty of
//      MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
//      GNU General Public License for more details.

//      You should have received a copy of the GNU General Public License
//      along with this program.  If not, see <https://www.gnu.org/licenses/>.

//! Favourite verses and their notes, kept in
//! `~/.local/share/Roma/data/fav_verses.json`.
//!
//! Books are stored by name, not by position, so a saved verse still points at
//! the right words when the Bible text is updated.

use std::cell::RefCell;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// One saved verse, with a note if one was written.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Favourite {
    pub book: String,
    pub chapter: u32,
    pub verse: u32,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub note: String,
}

impl Favourite {
    /// `John 3:16`, for labels and messages.
    pub fn reference(&self) -> String {
        format!("{} {}:{}", self.book, self.chapter, self.verse)
    }

    fn key(&self) -> (&str, u32, u32) {
        (&self.book, self.chapter, self.verse)
    }
}

/// The file, as it sits on disk.
#[derive(Default, Serialize, Deserialize)]
struct File {
    #[serde(default)]
    verses: Vec<Favourite>,
}

/// The saved favourites, and the file they came from.
pub struct Favourites {
    path: PathBuf,
    items: RefCell<Vec<Favourite>>,
}

impl Favourites {
    /// Loads from `$ROMA_DATA_HOME`, then `$XDG_DATA_HOME`, then
    /// `~/.local/share`.
    pub fn load_default() -> Self {
        Self::load(default_path())
    }

    /// Loads from `path`. A missing or unreadable file starts an empty list;
    /// the file is written when something is first saved.
    pub fn load(path: PathBuf) -> Self {
        let items = fs::read_to_string(&path)
            .ok()
            .and_then(|text| serde_json::from_str::<File>(&text).ok())
            .map_or_else(Vec::new, |file| file.verses);
        Self {
            path,
            items: RefCell::new(items),
        }
    }

    /// Where this list is kept.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Every saved verse, in the order they were first starred.
    pub fn all(&self) -> Vec<Favourite> {
        self.items.borrow().clone()
    }

    pub fn len(&self) -> usize {
        self.items.borrow().len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.borrow().is_empty()
    }

    /// Whether this verse is starred.
    pub fn has(&self, book: &str, chapter: u32, verse: u32) -> bool {
        self.items
            .borrow()
            .iter()
            .any(|item| item.key() == (book, chapter, verse))
    }

    /// The note for this verse, if it has one.
    pub fn note(&self, book: &str, chapter: u32, verse: u32) -> Option<String> {
        self.items
            .borrow()
            .iter()
            .find(|item| item.key() == (book, chapter, verse))
            .map(|item| item.note.clone())
            .filter(|note| !note.is_empty())
    }

    /// Stars or unstars a verse, and says which it did.
    pub fn toggle(&self, book: &str, chapter: u32, verse: u32) -> bool {
        let starred = !self.has(book, chapter, verse);
        if starred {
            self.items.borrow_mut().push(Favourite {
                book: book.to_string(),
                chapter,
                verse,
                note: String::new(),
            });
        } else {
            self.items
                .borrow_mut()
                .retain(|item| item.key() != (book, chapter, verse));
        }
        self.write();
        starred
    }

    /// Sets or clears the note of a starred verse. An empty note removes it.
    pub fn set_note(&self, book: &str, chapter: u32, verse: u32, note: &str) {
        {
            let mut items = self.items.borrow_mut();
            match items
                .iter_mut()
                .find(|item| item.key() == (book, chapter, verse))
            {
                Some(item) => item.note = note.trim().to_string(),
                None => items.push(Favourite {
                    book: book.to_string(),
                    chapter,
                    verse,
                    note: note.trim().to_string(),
                }),
            }
        }
        self.write();
    }

    /// Writes the file, beside itself first, so a crash cannot leave a
    /// half-written list.
    fn write(&self) {
        let file = File {
            verses: self.items.borrow().clone(),
        };
        let Ok(text) = serde_json::to_string_pretty(&file) else {
            return;
        };
        if let Some(parent) = self.path.parent()
            && let Err(error) = fs::create_dir_all(parent)
        {
            eprintln!("roma: cannot make {}: {error}", parent.display());
            return;
        }
        let temporary = self.path.with_extension("json.tmp");
        if let Err(error) = fs::write(&temporary, text) {
            eprintln!("roma: cannot write {}: {error}", temporary.display());
            return;
        }
        if let Err(error) = fs::rename(&temporary, &self.path) {
            eprintln!("roma: cannot save {}: {error}", self.path.display());
            let _ = fs::remove_file(&temporary);
        }
    }
}

/// `~/.local/share/Roma/data/fav_verses.json`.
pub fn default_path() -> PathBuf {
    let mut base: PathBuf = std::env::var_os("ROMA_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("XDG_DATA_HOME").map(PathBuf::from))
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/share")))
        .unwrap_or_default();
    if base.as_os_str().is_empty() {
        return PathBuf::from("fav_verses.json");
    }
    base.push("Roma/data");
    base.push("fav_verses.json");
    base
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!("roma-test-{name}/fav_verses.json"));
        let _ = fs::remove_dir_all(path.parent().unwrap());
        path
    }

    #[test]
    fn stars_and_notes_survive_a_reload() {
        let path = scratch("reload");
        let favourites = Favourites::load(path.clone());
        assert!(favourites.is_empty());

        assert!(favourites.toggle("John", 3, 16));
        favourites.set_note("John", 3, 16, "For everyone who believes.");

        assert!(favourites.has("John", 3, 16));
        assert!(!favourites.has("John", 3, 15));
        assert_eq!(
            favourites.note("John", 3, 16).as_deref(),
            Some("For everyone who believes.")
        );

        let reloaded = Favourites::load(path.clone());
        assert_eq!(reloaded.len(), 1);
        assert_eq!(
            reloaded.all()[0],
            Favourite {
                book: "John".into(),
                chapter: 3,
                verse: 16,
                note: "For everyone who believes.".into(),
            }
        );
        assert!(path.is_file(), "the file is where it was asked for");
    }

    #[test]
    fn unstarring_removes_the_verse_and_its_note() {
        let favourites = Favourites::load(scratch("unstarring"));
        favourites.toggle("Psalms", 23, 1);
        favourites.set_note("Psalms", 23, 1, "The shepherd");

        assert!(!favourites.toggle("Psalms", 23, 1));
        assert!(favourites.is_empty());
        assert_eq!(favourites.note("Psalms", 23, 1), None);
    }

    #[test]
    fn a_broken_file_starts_empty_rather_than_failing() {
        let path = scratch("broken");
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, "{ not json").unwrap();
        assert!(Favourites::load(path).is_empty());
    }
}
