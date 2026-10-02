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

//! The reader window: a libadwaita shell with a search bar, a book and chapter
//! browser, and the chapter text.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;
use std::time::Duration;

use gtk::prelude::*;
use gtk::{Application, gio, glib};
use libadwaita as adw;
use libadwaita::prelude::*;

use crate::bible::{Bible, Hit, Query, Results, Verse};

const SEARCH_DELAY: Duration = Duration::from_millis(120);
const SIDEBAR_WIDTH: i32 = 320;
const READING_WIDTH: i32 = 660;
const INVALID_POSITION: u32 = gtk::INVALID_LIST_POSITION;

/// A row of the book list: either a testament heading or a book.
enum BookRow {
    Section,
    Book(u32),
}

pub struct MainWindow {
    window: adw::ApplicationWindow,
    toast: adw::ToastOverlay,
    split: adw::OverlaySplitView,
    bible: Rc<Bible>,

    // Header bar
    sidebar_toggle: gtk::ToggleButton,
    window_title: adw::WindowTitle,
    search_entry: gtk::SearchEntry,

    // Footer bar
    previous: gtk::Button,
    next: gtk::Button,
    footer: gtk::Label,

    // Sidebar
    stack: gtk::Stack,
    books_list: gtk::ListView,
    books_model: gio::ListStore,
    books_selection: gtk::SingleSelection,
    book_rows: RefCell<Vec<BookRow>>,
    chapter_chips: RefCell<Vec<gtk::ToggleButton>>,
    chips_box: gtk::Box,
    results_heading: gtk::Label,
    results_stack: gtk::Stack,
    results_list: gtk::ListView,
    results_model: gio::ListStore,
    results_selection: gtk::SingleSelection,
    results_empty: adw::StatusPage,
    results: RefCell<Vec<Hit>>,

    // Reading pane
    eyebrow: gtk::Label,
    title: gtk::Label,
    divider: gtk::Box,
    empty_page: adw::StatusPage,
    verses: gtk::Box,
    scroller: gtk::ScrolledWindow,
    verse_rows: RefCell<HashMap<u32, gtk::Box>>,

    book: Cell<u32>,
    chapter: Cell<u32>,
    highlighted: Cell<u32>,
    /// Set while widgets are updated by code, to avoid feedback loops.
    syncing: Cell<bool>,
    search_timer: RefCell<Option<glib::SourceId>>,
}

/// Stylesheet for the reader window.
pub const STYLE: &str = include_str!("style.css");

/// Installs [`STYLE`] on the current display.
pub fn load_style() {
    let style = gtk::CssProvider::new();
    style.load_from_data(STYLE);
    if let Some(display) = gtk::gdk::Display::default() {
        gtk::style_context_add_provider_for_display(
            &display,
            &style,
            gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
    }
}

impl MainWindow {
    pub fn new(app: &Application, bible: Rc<Bible>) -> Rc<Self> {
        let window = adw::ApplicationWindow::builder()
            .application(app)
            .title("Roma")
            .default_width(1120)
            .default_height(820)
            .build();

        let me = Rc::new(Self {
            bible,
            window,
            toast: adw::ToastOverlay::new(),
            split: adw::OverlaySplitView::new(),
            sidebar_toggle: gtk::ToggleButton::new(),
            window_title: adw::WindowTitle::new("Roma", ""),
            search_entry: gtk::SearchEntry::new(),
            previous: gtk::Button::from_icon_name("go-up-symbolic"),
            next: gtk::Button::from_icon_name("go-down-symbolic"),
            footer: gtk::Label::new(None),
            stack: gtk::Stack::new(),
            books_list: gtk::ListView::new(
                None::<gtk::SingleSelection>,
                None::<gtk::SignalListItemFactory>,
            ),
            books_model: gio::ListStore::new::<gtk::StringObject>(),
            books_selection: gtk::SingleSelection::new(None::<gio::ListModel>),
            book_rows: RefCell::new(Vec::new()),
            chapter_chips: RefCell::new(Vec::new()),
            chips_box: gtk::Box::new(gtk::Orientation::Horizontal, 6),
            results_heading: gtk::Label::new(Some("Results")),
            results_stack: gtk::Stack::new(),
            results_list: gtk::ListView::new(
                None::<gtk::SingleSelection>,
                None::<gtk::SignalListItemFactory>,
            ),
            results_model: gio::ListStore::new::<gtk::StringObject>(),
            results_selection: gtk::SingleSelection::new(None::<gio::ListModel>),
            results_empty: adw::StatusPage::new(),
            results: RefCell::new(Vec::new()),
            eyebrow: gtk::Label::new(None),
            title: gtk::Label::new(None),
            divider: gtk::Box::new(gtk::Orientation::Horizontal, 0),
            empty_page: adw::StatusPage::new(),
            verses: gtk::Box::new(gtk::Orientation::Vertical, 0),
            scroller: gtk::ScrolledWindow::new(),
            verse_rows: RefCell::new(HashMap::new()),
            book: Cell::new(0),
            chapter: Cell::new(1),
            highlighted: Cell::new(0),
            syncing: Cell::new(false),
            search_timer: RefCell::new(None),
        });

        me.build();
        me
    }

    /// Shows the window, bringing it to the front if it is already open.
    pub fn present(&self) {
        self.window.present();
    }

    /// The window holding the reader.
    pub fn window(&self) -> &adw::ApplicationWindow {
        &self.window
    }

    /// Title of the chapter on screen, e.g. `John 3`.
    pub fn chapter_title(&self) -> String {
        self.bible
            .chapter_reference(self.book.get(), self.chapter.get())
    }

    /// Number of verses rendered for the current chapter.
    pub fn shown_verses(&self) -> usize {
        self.verse_rows.borrow().len()
    }

    /// The verse scrolled to and highlighted, if any.
    pub fn highlighted_verse(&self) -> Option<u32> {
        match self.highlighted.get() {
            0 => None,
            verse => Some(verse),
        }
    }

    /// The results currently listed in the sidebar.
    pub fn results(&self) -> Vec<Hit> {
        self.results.borrow().clone()
    }

    /// Runs a search straight away, as the search bar does after a pause.
    pub fn search(self: &Rc<Self>, text: &str) {
        self.cancel_pending_search();
        if text.trim().is_empty() {
            self.clear_search();
        } else {
            self.run_search(text);
        }
    }

    /// Drops the current search and returns the sidebar to the book browser.
    pub fn clear_search(self: &Rc<Self>) {
        self.clear_results();
        self.search_entry.set_text("");
    }

    /// Follows a query: a passage opens, words are searched.
    pub fn jump(self: &Rc<Self>, text: &str) {
        match self.bible.parse_query(text) {
            Some(Query::Passage {
                book,
                chapters,
                verses,
            }) => {
                let chapter = chapters.first().copied().unwrap_or(1);
                let verse = verses.and_then(|verses| verses.first().copied());
                self.go_to(book, chapter, verse);
            }
            Some(Query::Text(_)) => self.run_search(text),
            None => self.clear_search(),
        }
    }

    /// Opens a chapter, optionally scrolling to and highlighting a verse.
    pub fn go_to(self: &Rc<Self>, book: u32, chapter: u32, verse: Option<u32>) {
        let known = self
            .bible
            .book(book)
            .is_some_and(|book| book.chapter(chapter).is_some());
        if !known {
            let name = self
                .bible
                .book(book)
                .map_or_else(|| "the bible".to_string(), |book| book.name.clone());
            self.notify(format!("{name} has no chapter {chapter}"));
            return;
        }

        self.syncing.set(true);
        if book != self.book.get() {
            self.fill_chapters(book);
        }
        self.book.set(book);
        self.chapter.set(chapter);
        self.books_selection.set_selected(self.book_row(book));
        self.select_chapter_chip(chapter);
        self.syncing.set(false);

        self.render_chapter(book, chapter);

        if let Some(verse) = verse {
            self.scroll_to_verse(verse);
        }
    }

    // ----------------------------------------------------------------- layout

    fn build(self: &Rc<Self>) {
        let header = self.build_header();
        let footer = self.build_footer();
        let sidebar = self.build_sidebar();
        let reader = self.build_reader();

        let toolbar = adw::ToolbarView::new();
        toolbar.add_top_bar(&header);
        toolbar.set_content(Some(&reader));
        toolbar.add_bottom_bar(&footer);

        self.split.set_sidebar(Some(&sidebar));
        self.split.set_content(Some(&toolbar));
        self.split.set_sidebar_width_fraction(0.32);
        self.split.set_vexpand(true);

        self.toast.set_child(Some(&self.split));
        self.window.set_content(Some(&self.toast));

        self.init_lists();
        self.connect_signals();
        self.fill_books();
        self.go_to(0, 1, None);
    }

    fn build_header(self: &Rc<Self>) -> adw::HeaderBar {
        let header = adw::HeaderBar::new();

        self.sidebar_toggle.set_icon_name("view-sidebar-symbolic");
        self.sidebar_toggle
            .set_tooltip_text(Some("Show or hide the books (F9)"));
        self.sidebar_toggle.add_css_class("flat");
        header.pack_start(&self.sidebar_toggle);

        self.window_title.set_title("Roma");
        header.set_title_widget(Some(&self.window_title));

        self.search_entry
            .set_placeholder_text(Some("Search verses, or type John 3:16"));
        self.search_entry.add_css_class("search-field");
        self.search_entry.set_width_chars(22);
        self.search_entry.set_margin_top(6);
        self.search_entry.set_margin_bottom(6);
        self.search_entry.set_margin_end(6);
        header.pack_end(&self.search_entry);

        let me = self.clone();
        self.sidebar_toggle
            .connect_toggled(move |toggle| me.set_sidebar(toggle.is_active()));

        for (button, step, tip, action) in [
            (&self.previous, -1, "Previous chapter (Alt+Up, Left)", "previous-chapter"),
            (&self.next, 1, "Next chapter (Alt+Down, Right)", "next-chapter"),
        ] {
            button.set_tooltip_text(Some(tip));
            button.add_css_class("chapter-button");
            button.set_action_name(Some(&format!("win.{action}")));
            let me = self.clone();
            let step = step as i32;
            button.connect_clicked(move |_| me.step_chapter(step));
        }

        let me = self.clone();
        self.search_entry
            .connect_search_changed(move |entry| me.on_search_changed(entry.text().as_str()));
        let me = self.clone();
        self.search_entry.connect_activate(move |_| {
            let me = me.clone();
            me.open_first_result();
        });

        header
    }

    fn build_sidebar(self: &Rc<Self>) -> gtk::Widget {
        let sidebar = gtk::Box::new(gtk::Orientation::Vertical, 0);

        sidebar.append(&scroller(&self.books_list));

        sidebar.append(&gtk::Separator::new(gtk::Orientation::Horizontal));

        let chips_label = section_label("Chapters");
        sidebar.append(&chips_label);

        self.chips_box.set_margin_top(2);
        self.chips_box.set_margin_bottom(12);
        self.chips_box.set_margin_start(12);
        self.chips_box.set_margin_end(12);
        let chips = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Automatic)
            .vscrollbar_policy(gtk::PolicyType::Never)
            .child(&self.chips_box)
            .build();
        chips.set_vexpand(false);
        sidebar.append(&chips);

        self.results_heading.set_margin_top(12);
        self.results_heading.set_margin_bottom(4);
        self.results_heading.set_margin_start(16);
        self.results_heading.set_margin_end(16);
        self.results_heading.add_css_class("sidebar-heading");
        self.results_heading.set_xalign(0.0);

        self.results_empty
            .set_icon_name(Some("system-search-symbolic"));
        self.results_empty.set_title("No results");
        self.results_empty
            .set_description(Some("Try fewer words, or a reference like John 3:16."));
        self.results_empty.set_vexpand(true);

        self.results_stack
            .add_named(&scroller(&self.results_list), Some("list"));
        self.results_stack
            .add_named(&self.results_empty, Some("empty"));
        self.results_stack.set_visible_child_name("list");
        self.results_stack.set_vexpand(true);

        let results = gtk::Box::new(gtk::Orientation::Vertical, 0);
        results.append(&self.results_heading);
        results.append(&self.results_stack);

        self.stack.add_named(&sidebar, Some("browser"));
        self.stack.add_named(&results, Some("results"));
        self.stack.set_visible_child_name("browser");
        self.stack.set_vexpand(true);

        let wrapper = gtk::Box::new(gtk::Orientation::Vertical, 0);
        wrapper.append(&self.stack);
        wrapper.set_size_request(SIDEBAR_WIDTH, -1);
        wrapper.upcast()
    }

    fn build_reader(self: &Rc<Self>) -> gtk::Widget {
        // Document header: section, then the chapter title as a heading.
        self.eyebrow.add_css_class("eyebrow");
        self.eyebrow.set_xalign(0.0);

        self.title.add_css_class("title-1");
        self.title.set_xalign(0.0);
        self.title.set_wrap(true);
        self.title.set_selectable(true);
        self.title.set_margin_top(2);

        self.divider.add_css_class("divider");
        self.divider.set_margin_top(18);
        self.divider.set_margin_bottom(6);

        self.empty_page.set_icon_name(Some("book-x-symbolic"));
        self.empty_page.set_title("Nothing to read here");
        self.empty_page
            .set_description(Some("This passage has no text in this translation."));
        self.empty_page.set_vexpand(true);

        let document = gtk::Box::new(gtk::Orientation::Vertical, 0);
        document.set_margin_top(28);
        document.set_margin_bottom(28);
        document.set_margin_start(16);
        document.set_margin_end(16);
        document.append(&self.eyebrow);
        document.append(&self.title);
        document.append(&self.divider);
        document.append(&self.verses);
        document.append(&self.empty_page);

        let clamp = adw::Clamp::new();
        clamp.set_maximum_size(READING_WIDTH);
        clamp.set_tightening_threshold(READING_WIDTH - 80);
        clamp.set_child(Some(&document));

        self.scroller
            .set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
        self.scroller.set_vexpand(true);
        self.scroller.set_child(Some(&clamp));

        let reader = gtk::Box::new(gtk::Orientation::Vertical, 0);
        reader.append(&self.scroller);
        reader.upcast()
    }

    /// The bottom bar: previous and next chapter, with where we are.
    fn build_footer(self: &Rc<Self>) -> gtk::Widget {
        self.footer.add_css_class("document-footer");
        self.footer.set_hexpand(true);

        let bar = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        bar.set_margin_top(8);
        bar.set_margin_bottom(10);
        bar.set_margin_start(12);
        bar.set_margin_end(12);
        bar.append(&self.previous);
        bar.append(&self.footer);
        bar.append(&self.next);
        bar.upcast()
    }

    fn init_lists(self: &Rc<Self>) {
        let books_factory = gtk::SignalListItemFactory::new();
        books_factory.connect_setup(|_, item| setup_row(item));
        let me = self.clone();
        books_factory.connect_bind(move |_, item| {
            let me = me.clone();
            let Some(label) = row_label(item) else {
                return;
            };
            let rows = me.book_rows.borrow();
            let class = match rows.get(item.position() as usize) {
                Some(BookRow::Book(_)) => "book",
                _ => "book-section",
            };
            drop(rows);
            label.set_css_classes(&[class]);
            label.set_text(&row_text(item));
        });

        let results_factory = gtk::SignalListItemFactory::new();
        results_factory.connect_setup(|_, item| setup_result_row(item));
        let me = self.clone();
        results_factory.connect_bind(move |_, item| {
            let me = me.clone();
            let (reference, snippet) = {
                let results = me.results.borrow();
                let Some(hit) = results.get(item.position() as usize) else {
                    return;
                };
                (
                    me.bible.reference(hit.book, hit.chapter, hit.verse),
                    hit.snippet.clone(),
                )
            };
            let Some(row) = item.child().and_downcast::<gtk::Box>() else {
                return;
            };
            if let Some(label) = row.first_child().and_downcast::<gtk::Label>() {
                label.set_text(&reference);
            }
            if let Some(label) = row.last_child().and_downcast::<gtk::Label>() {
                label.set_text(&snippet);
            }
        });

        self.books_selection.set_model(Some(&self.books_model));
        self.books_list.set_model(Some(&self.books_selection));
        self.books_list.set_factory(Some(&books_factory));

        self.results_selection.set_model(Some(&self.results_model));
        self.results_selection.set_can_unselect(false);
        self.results_list.set_model(Some(&self.results_selection));
        self.results_list.set_factory(Some(&results_factory));
    }

    fn connect_signals(self: &Rc<Self>) {
        let me = self.clone();
        self.books_selection
            .connect_selected_notify(move |selection| {
                let me = me.clone();
                me.on_book_selected(selection.selected());
            });

        let me = self.clone();
        self.results_selection
            .connect_selected_notify(move |selection| {
                let me = me.clone();
                me.on_result_selected(selection.selected());
            });

        let me = self.clone();
        self.split.connect_collapsed_notify(move |split| {
            let me = me.clone();
            me.sidebar_toggle.set_active(!split.is_collapsed());
        });

        let keys = gtk::EventControllerKey::new();
        keys.set_propagation_phase(gtk::PropagationPhase::Capture);
        let me = self.clone();
        keys.connect_key_pressed(move |_, key, _, _| {
            if me.search_entry.has_focus() {
                return glib::Propagation::Proceed;
            }
            match key {
                gtk::gdk::Key::Left | gtk::gdk::Key::KP_Left => me.step_chapter(-1),
                gtk::gdk::Key::Right | gtk::gdk::Key::KP_Right => me.step_chapter(1),
                _ => return glib::Propagation::Proceed,
            }
            glib::Propagation::Stop
        });
        self.window.add_controller(keys);

        for (name, accels) in [
            ("search", ["<Control>f"].as_slice()),
            ("browser", ["F9"].as_slice()),
            ("next-chapter", ["<Alt>Down"].as_slice()),
            ("previous-chapter", ["<Alt>Up"].as_slice()),
            ("about", ["F1"].as_slice()),
        ] {
            let action = gio::SimpleAction::new(name, None);
            let me = self.clone();
            action.connect_activate(move |_, _| me.run_action(name));
            self.window.add_action(&action);
            if let Some(app) = self.window.application() {
                app.set_accels_for_action(&format!("win.{name}"), accels);
            }
        }
    }

    fn run_action(self: &Rc<Self>, name: &str) {
        match name {
            "search" => {
                self.set_sidebar(true);
                self.search_entry.grab_focus();
            }
            "browser" => self.set_sidebar(self.split.is_collapsed()),
            "next-chapter" => self.step_chapter(1),
            "previous-chapter" => self.step_chapter(-1),
            "about" => self.show_about(),
            _ => {}
        }
    }

    fn set_sidebar(&self, visible: bool) {
        self.sidebar_toggle.set_active(visible);
        self.split.set_collapsed(!visible);
    }

    fn notify(&self, message: impl Into<String>) {
        self.toast.add_toast(adw::Toast::new(&message.into()));
    }

    fn show_about(&self) {
        let about = adw::AboutDialog::builder()
            .application_name("Roma")
            .application_icon("view-book-symbolic")
            .version(env!("CARGO_PKG_VERSION"))
            .developer_name("Roma contributors")
            .comments("A Bible reader for the desktop.")
            .license_type(gtk::License::Gpl30)
            .build();
        about.present(Some(&self.window));
    }

    // ------------------------------------------------------------------ lists

    fn fill_books(self: &Rc<Self>) {
        // Collect first: appending to the model runs the bind callbacks.
        let mut rows = Vec::new();
        let mut labels = Vec::new();
        let mut section = "";
        for (index, book) in self.bible.books().iter().enumerate() {
            if book.section != section {
                section = book.section;
                rows.push(BookRow::Section);
                labels.push(section.to_uppercase());
            }
            rows.push(BookRow::Book(index as u32));
            labels.push(book.name.clone());
        }

        *self.book_rows.borrow_mut() = rows;
        self.books_model.remove_all();
        for label in &labels {
            self.books_model.append(&gtk::StringObject::new(label));
        }

        self.fill_chapters(self.book.get());
    }

    fn fill_chapters(self: &Rc<Self>, book: u32) {
        let me = self.clone();
        while let Some(child) = self.chips_box.first_child() {
            self.chips_box.remove(&child);
        }
        let mut chips = self.chapter_chips.borrow_mut();
        chips.clear();

        let Some(contents) = self.bible.book(book) else {
            return;
        };
        for chapter in &contents.chapters {
            let chip = gtk::ToggleButton::with_label(&chapter.number.to_string());
            chip.add_css_class("chip");

            let number = chapter.number;
            let me = me.clone();
            chip.connect_clicked(move |_| {
                if !me.syncing.get() {
                    me.go_to(book, number, None);
                }
            });

            self.chips_box.append(&chip);
            chips.push(chip);
        }
    }

    fn select_chapter_chip(&self, chapter: u32) {
        for (index, chip) in self.chapter_chips.borrow().iter().enumerate() {
            chip.set_active(index as u32 + 1 == chapter);
        }
    }

    fn book_row(&self, book: u32) -> u32 {
        self.book_rows
            .borrow()
            .iter()
            .position(|row| matches!(row, BookRow::Book(index) if *index == book))
            .unwrap_or(INVALID_POSITION as usize) as u32
    }

    fn on_book_selected(self: &Rc<Self>, position: u32) {
        if self.syncing.get() {
            return;
        }
        let book = {
            let rows = self.book_rows.borrow();
            match rows.get(position as usize) {
                Some(BookRow::Book(book)) => *book,
                _ => return,
            }
        };
        let first = self
            .bible
            .book(book)
            .and_then(|book| book.chapters.first().map(|chapter| chapter.number));
        if let Some(chapter) = first {
            self.go_to(book, chapter, None);
        }
    }

    fn on_result_selected(self: &Rc<Self>, position: u32) {
        if self.syncing.get() {
            return;
        }
        let hit = self.results.borrow().get(position as usize).cloned();
        if let Some(hit) = hit {
            self.go_to(hit.book, hit.chapter, Some(hit.verse));
            self.scroll_list_to(&self.results_list, position);
        }
    }

    // --------------------------------------------------------------- reading

    fn step_chapter(self: &Rc<Self>, delta: i32) {
        let (book, chapter) = (self.book.get(), self.chapter.get());
        let target = if delta < 0 {
            self.bible.previous_chapter(book, chapter)
        } else {
            self.bible.next_chapter(book, chapter)
        };
        if let Some((book, chapter)) = target {
            self.go_to(book, chapter, None);
        }
    }

    fn render_chapter(&self, book: u32, chapter: u32) {
        while let Some(child) = self.verses.first_child() {
            self.verses.remove(&child);
        }
        self.verse_rows.borrow_mut().clear();
        self.highlighted.set(0);

        let title = self.bible.chapter_reference(book, chapter);
        let section = self
            .bible
            .book(book)
            .map_or("", |book| book.section)
            .to_uppercase();

        self.title.set_text(&title);
        self.eyebrow.set_text(&section);
        self.window_title.set_title(&title);
        self.window_title.set_subtitle(&section);

        let mut shown = 0;
        for verse in self
            .bible
            .chapter_verses(book, chapter)
            .iter()
            .filter(|verse| !verse.text.is_empty())
        {
            self.verses.append(&self.verse_row(verse));
            shown += 1;
        }

        self.empty_page.set_visible(shown == 0);
        self.divider.set_visible(shown > 0);
        self.footer.set_text(&format!(
            "{title} \u{b7} {shown} verse{}",
            plural(shown)
        ));
    }

    fn verse_row(&self, verse: &Verse) -> gtk::Box {
        let row = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        row.add_css_class("verse");

        let number = gtk::Label::new(Some(&verse.number.to_string()));
        number.add_css_class("verse-number");
        number.set_xalign(1.0);
        number.set_width_chars(3);

        let text = gtk::Label::new(Some(&verse.text));
        text.add_css_class("verse-text");
        text.set_wrap(true);
        text.set_hexpand(true);
        text.set_xalign(0.0);
        text.set_selectable(true);

        row.append(&number);
        row.append(&text);
        self.verse_rows
            .borrow_mut()
            .insert(verse.number, row.clone());
        row
    }

    // ----------------------------------------------------------------- search

    fn on_search_changed(self: &Rc<Self>, text: &str) {
        self.cancel_pending_search();
        if text.trim().is_empty() {
            self.clear_results();
            return;
        }

        let me = self.clone();
        let text = text.to_string();
        let timer = glib::timeout_add_local_once(SEARCH_DELAY, move || {
            // The timer is spent by now: forget it so it is not removed twice.
            *me.search_timer.borrow_mut() = None;
            me.run_search(&text);
        });
        *self.search_timer.borrow_mut() = Some(timer);
    }

    fn cancel_pending_search(&self) {
        if let Some(timer) = self.search_timer.borrow_mut().take() {
            timer.remove();
        }
    }

    fn run_search(&self, text: &str) {
        let Some(query) = self.bible.parse_query(text) else {
            self.clear_results();
            return;
        };
        let results = self.bible.search(&query);

        let summary = results_summary(&results);
        let references: Vec<String> = results
            .hits
            .iter()
            .map(|hit| self.bible.reference(hit.book, hit.chapter, hit.verse))
            .collect();

        *self.results.borrow_mut() = results.hits;
        self.results_model.remove_all();
        for reference in &references {
            self.results_model
                .append(&gtk::StringObject::new(reference));
        }

        self.stack.set_visible_child_name("results");
        self.results_heading.set_text(&summary);
        self.results_stack
            .set_visible_child_name(if self.results.borrow().is_empty() {
                "empty"
            } else {
                "list"
            });

        self.syncing.set(true);
        self.results_selection.set_selected(0);
        self.syncing.set(false);
    }

    fn open_first_result(self: &Rc<Self>) {
        if let Some(hit) = self.results.borrow().first().cloned() {
            self.go_to(hit.book, hit.chapter, Some(hit.verse));
        }
    }

    fn clear_results(&self) {
        self.results_model.remove_all();
        self.results.borrow_mut().clear();
        self.results_heading.set_text("Results");
        self.stack.set_visible_child_name("browser");
    }

    // -------------------------------------------------------------- scrolling

    fn scroll_to_verse(&self, verse: u32) {
        let Some(row) = self.verse_rows.borrow().get(&verse).cloned() else {
            self.notify(format!("Verse {verse} is not in this chapter"));
            return;
        };
        if let Some(previous) = self
            .verse_rows
            .borrow()
            .get(&self.highlighted.get())
            .cloned()
        {
            previous.remove_css_class("highlighted");
        }
        row.add_css_class("highlighted");
        self.highlighted.set(verse);
        self.scroll_content_to(&row);
    }

    fn scroll_content_to(&self, target: &impl IsA<gtk::Widget>) {
        let adjustment = self.scroller.vadjustment();
        let Some(container) = self.scroller.child() else {
            return;
        };
        if let Some((_, y)) = target.translate_coordinates(&container, 0.0, 0.0) {
            let offset = adjustment.value() + y - adjustment.page_size() * 0.25;
            adjustment.set_value(
                offset
                    .max(adjustment.lower())
                    .min(adjustment.upper() - adjustment.page_size()),
            );
        }
    }

    fn scroll_list_to(&self, list: &gtk::ListView, position: u32) {
        let Some(adjustment) = list.vadjustment() else {
            return;
        };
        let mut child = list.first_child();
        let mut index = 0;
        while let Some(row) = child {
            if index >= position {
                if let Some((_, y)) = row.translate_coordinates(list, 0.0, 0.0) {
                    let offset = adjustment.value() + y - adjustment.page_size() * 0.25;
                    adjustment.set_value(
                        offset
                            .max(adjustment.lower())
                            .min(adjustment.upper() - adjustment.page_size()),
                    );
                }
                return;
            }
            index += 1;
            child = row.next_sibling();
        }
    }
}

fn plural(count: usize) -> &'static str {
    if count == 1 { "" } else { "s" }
}

fn results_summary(results: &Results) -> String {
    use crate::bible::MAX_HITS;
    if results.hits.is_empty() {
        return "Results · none".to_string();
    }
    if results.total > MAX_HITS {
        return format!("Results · first {MAX_HITS} of {}", results.total);
    }
    format!("Results · {}", results.total)
}

fn section_label(text: &str) -> gtk::Label {
    let label = gtk::Label::new(Some(text.to_uppercase().as_str()));
    label.add_css_class("sidebar-heading");
    label.set_xalign(0.0);
    label.set_margin_top(12);
    label.set_margin_bottom(6);
    label.set_margin_start(16);
    label.set_margin_end(16);
    label
}

fn setup_row(item: &gtk::ListItem) {
    let label = gtk::Label::new(None);
    label.set_xalign(0.0);
    label.set_ellipsize(gtk::pango::EllipsizeMode::End);
    item.set_child(Some(&label));
}

fn setup_result_row(item: &gtk::ListItem) {
    let row = gtk::Box::new(gtk::Orientation::Vertical, 0);
    row.add_css_class("result");

    let reference = gtk::Label::new(None);
    reference.add_css_class("result-reference");
    reference.set_xalign(0.0);
    reference.set_ellipsize(gtk::pango::EllipsizeMode::End);

    let snippet = gtk::Label::new(None);
    snippet.add_css_class("result-snippet");
    snippet.set_xalign(0.0);
    snippet.set_ellipsize(gtk::pango::EllipsizeMode::End);

    row.append(&reference);
    row.append(&snippet);
    item.set_child(Some(&row));
}

fn row_label(item: &gtk::ListItem) -> Option<gtk::Label> {
    item.child().and_downcast::<gtk::Label>()
}

fn row_text(item: &gtk::ListItem) -> String {
    item.item()
        .and_downcast::<gtk::StringObject>()
        .map_or_else(String::new, |text| text.string().to_string())
}

fn scroller(list: &gtk::ListView) -> gtk::ScrolledWindow {
    list.set_vexpand(true);
    gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .vexpand(true)
        .child(list)
        .build()
}
