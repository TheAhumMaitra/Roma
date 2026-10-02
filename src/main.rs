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

use std::cell::OnceCell;
use std::rc::Rc;

use gtk::Application;
use gtk::glib;
use gtk::prelude::*;
use libadwaita as adw;

use roma::{MainWindow, bible::Bible, ui};

const APP_ID: &str = "dev.roma.bible";

fn main() -> glib::ExitCode {
    let app = Application::builder().application_id(APP_ID).build();
    app.connect_startup(|_| {
        adw::init().expect("libadwaita initialises");
        ui::load_style();
    });

    let Some(path) = roma::locate_bible() else {
        eprintln!(
            "roma: no bible text found; pass a path to {}",
            roma::BIBLE_FILE
        );
        return glib::ExitCode::FAILURE;
    };
    let bible = match Bible::load(&path) {
        Ok(bible) => Rc::new(bible),
        Err(error) => {
            eprintln!("roma: {error}");
            return glib::ExitCode::FAILURE;
        }
    };
    eprintln!(
        "roma: {} books, {} verses from {}",
        bible.book_count(),
        bible.verse_count(),
        path.display()
    );

    let window: Rc<OnceCell<Rc<MainWindow>>> = Rc::new(OnceCell::new());
    app.connect_activate(move |app| {
        if let Some(window) = window.get() {
            window.present();
            return;
        }
        let reader = MainWindow::new(app, bible.clone());
        window.set(reader.clone()).ok();
        reader.present();
    });

    app.run()
}
