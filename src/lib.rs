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

//! The library holds everything that can be reused: the bible text and its
//! queries in [`bible`], the widgets and styling in [`ui`], and locating the
//! text file on disk.

pub mod bible;
pub mod help;
pub mod store;
pub mod ui;

use std::path::{Path, PathBuf};

pub use bible::{Bible, Hit, Query, Results, Verse};
pub use help::present as show_help;
pub use store::Favourites;
pub use ui::{MainWindow, STYLE, load_style};

/// File name of the bible text shipped in `sources/`.
pub const BIBLE_FILE: &str = "sources/CPDV.json";

/// Finds the bible text: a path from the command line, then `ROMA_BIBLE`, then
/// the usual places next to the executable, then the source tree.
pub fn locate_bible() -> Option<PathBuf> {
    let mut candidates: Vec<PathBuf> = Vec::new();

    if let Some(argument) = std::env::args_os().nth(1)
        && !argument.to_string_lossy().starts_with('-')
    {
        candidates.push(PathBuf::from(argument));
    }
    if let Ok(path) = std::env::var("ROMA_BIBLE") {
        candidates.push(PathBuf::from(path));
    }
    if let Ok(executable) = std::env::current_exe() {
        let directory = executable.parent().unwrap_or(Path::new("."));
        for depth in 0..3 {
            candidates.push(directory.join(BIBLE_FILE));
            for _ in 0..depth {
                let Some(up) = directory.parent() else { break };
                candidates.push(up.join(BIBLE_FILE));
            }
        }
    }
    candidates.push(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(BIBLE_FILE));

    candidates.into_iter().find(|path| path.is_file())
}
