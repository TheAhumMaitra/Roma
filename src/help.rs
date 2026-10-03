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

//! The help window: what Roma is, who made it, which keys work, and the
//! licence it ships under.

use gtk::glib;
use gtk::prelude::*;
use libadwaita as adw;
use libadwaita::prelude::*;

/// The licence, verbatim, so the app carries its own copy.
const LICENCE: &str = include_str!("../LICENSE");
const NAME: &str = "Roma";
const VERSION: &str = env!("CARGO_PKG_VERSION");
const CREATOR: &str = env!("CARGO_PKG_AUTHORS");

/// Builds the help window for `parent` and shows it.
pub fn present(parent: &impl IsA<gtk::Window>) -> adw::Window {
    let window = build(parent);
    window.present();
    window
}

pub(crate) fn build(parent: &impl IsA<gtk::Window>) -> adw::Window {
    let window = adw::Window::builder()
        .title(format!("About {NAME}"))
        .transient_for(parent)
        .modal(false)
        .default_width(440)
        .default_height(480)
        .build();

    let header = adw::HeaderBar::new();
    let title = adw::WindowTitle::new(NAME, VERSION);
    header.set_title_widget(Some(&title));

    let close = gtk::Button::with_label("Close");
    close.add_css_class("suggested-action");
    close.connect_clicked({
        let window = window.clone();
        move |_| window.close()
    });
    header.pack_end(&close);

    let scroller = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .vscrollbar_policy(gtk::PolicyType::Automatic)
        .vexpand(true)
        .build();

    scroller.set_child(Some(&body()));

    let toolbar = adw::ToolbarView::new();
    toolbar.add_top_bar(&header);
    toolbar.set_content(Some(&scroller));
    window.set_content(Some(&toolbar));

    // Escape closes, like every other window.
    let keys = gtk::EventControllerKey::new();
    keys.connect_key_pressed({
        let window = window.clone();
        move |_, key, _, _| {
            if key == gtk::gdk::Key::Escape {
                window.close();
            }
            glib::Propagation::Proceed
        }
    });
    window.add_controller(keys);

    window
}

/// The scrolling body: what it is, who made it, the keys, the licence.
fn body() -> gtk::Box {
    let body = gtk::Box::new(gtk::Orientation::Vertical, 0);
    body.set_margin_top(24);
    body.set_margin_bottom(24);
    body.set_margin_start(28);
    body.set_margin_end(28);

    let name = gtk::Label::new(Some(NAME));
    name.add_css_class("title-1");
    name.set_xalign(0.0);
    body.append(&name);

    let version = gtk::Label::new(Some(&format!("Version {VERSION}")));
    version.add_css_class("help-meta");
    version.set_xalign(0.0);
    body.append(&version);

    let creator = gtk::Label::new(Some(&format!("Made by {CREATOR}")));
    creator.add_css_class("help-meta");
    creator.set_xalign(0.0);
    creator.set_selectable(true);
    body.append(&creator);

    body.append(&divider());

    let which = gtk::Label::new(Some(
        "Free software, under the GNU General Public License, version 3 or later.",
    ));
    which.add_css_class("help-meta");
    which.set_xalign(0.0);
    which.set_wrap(true);
    body.append(&which);

    let text = gtk::Label::new(Some(LICENCE));
    text.add_css_class("licence-text");
    text.set_xalign(0.0);
    text.set_yalign(0.0);
    text.set_selectable(true);
    text.set_wrap(false);

    let licence = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Automatic)
        .vscrollbar_policy(gtk::PolicyType::Automatic)
        .vexpand(true)
        .child(&text)
        .build();
    licence.add_css_class("licence-frame");
    body.append(&licence);

    body
}

fn divider() -> gtk::Box {
    let divider = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    divider.add_css_class("divider");
    divider.set_margin_top(18);
    divider.set_margin_bottom(10);
    divider
}