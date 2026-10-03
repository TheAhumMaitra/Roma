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

use crate::bible::{Bible, Book, Hit, Query, Results, Verse};
use crate::store;

const SEARCH_DELAY: Duration = Duration::from_millis(120);
const SIDEBAR_WIDTH: i32 = 264;
/// Height of the chapter grid before it starts to scroll.
const CHIPS_HEIGHT: i32 = 122;
const READING_WIDTH: i32 = 720;
const INVALID_POSITION: u32 = gtk::INVALID_LIST_POSITION;
/// No verse picked.
const NO_VERSE: u32 = 0;
/// Said of him, coloured in every verse.
const NAME: &str = "Jesus";
/// The red of the stylesheet, for the spans that CSS cannot reach.
const ACCENT: &str = "#ff4d45";

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
    help_button: gtk::Button,
    footer: gtk::Label,

    // Sidebar
    stack: gtk::Stack,
    books_list: gtk::ListView,
    books_model: gio::ListStore,
    books_selection: gtk::SingleSelection,
    book_rows: RefCell<Vec<BookRow>>,
    chapter_chips: RefCell<Vec<gtk::ToggleButton>>,
    chips_box: gtk::FlowBox,
    results_heading: gtk::Label,
    results_stack: gtk::Stack,
    results_list: gtk::ListView,
    results_model: gio::ListStore,
    results_selection: gtk::SingleSelection,
    results_empty: adw::StatusPage,
    results: RefCell<Vec<Hit>>,

    // Favourites
    favourites: store::Favourites,
    favourites_button: gtk::Button,
    favourites_heading: gtk::Label,
    favourites_stack: gtk::Stack,
    favourites_list: gtk::ListView,
    favourites_model: gio::ListStore,
    favourites_selection: gtk::SingleSelection,
    favourites_rows: RefCell<Vec<store::Favourite>>,
    favourites_empty: adw::StatusPage,

    // The one place to keep or annotate the verse that is picked
    tools_revealer: gtk::Revealer,
    tools_reference: gtk::Label,
    tools_star: gtk::Button,
    tools_note: gtk::Button,

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
    selected_verse: Cell<u32>,
    help_window: RefCell<Option<adw::Window>>,
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
        Self::with_favourites(app, bible, store::Favourites::load_default())
    }

    /// As `new`, with a list of favourites from somewhere else. Tests use this
    /// to keep their verses out of the reader's own data.
    pub fn with_favourites(
        app: &Application,
        bible: Rc<Bible>,
        favourites: store::Favourites,
    ) -> Rc<Self> {
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
            previous: gtk::Button::with_label("\u{f100}"),
            next: gtk::Button::with_label("\u{f101}"),
            help_button: gtk::Button::new(),
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
            chips_box: gtk::FlowBox::new(),
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
            favourites,
            favourites_button: gtk::Button::new(),
            favourites_heading: gtk::Label::new(Some("Favourites")),
            favourites_stack: gtk::Stack::new(),
            favourites_list: gtk::ListView::new(
                None::<gtk::SingleSelection>,
                None::<gtk::SignalListItemFactory>,
            ),
            favourites_model: gio::ListStore::new::<gtk::StringObject>(),
            favourites_selection: gtk::SingleSelection::new(None::<gio::ListModel>),
            favourites_rows: RefCell::new(Vec::new()),
            favourites_empty: adw::StatusPage::new(),
            tools_revealer: gtk::Revealer::new(),
            tools_reference: gtk::Label::new(None),
            tools_star: gtk::Button::new(),
            tools_note: gtk::Button::new(),
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
            selected_verse: Cell::new(NO_VERSE),
            help_window: RefCell::new(None),
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

    /// True for the New Testament, where the text carries the words of Jesus.
    fn is_new_testament(&self, book: u32) -> bool {
        self.bible.book(book).is_some_and(Book::is_new_testament)
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
        let sidebar = self.build_sidebar();
        let reader = self.build_reader();

        let toolbar = adw::ToolbarView::new();
        toolbar.add_top_bar(&header);
        toolbar.set_content(Some(&reader));

        self.split.set_sidebar(Some(&sidebar));
        self.split.set_content(Some(&toolbar));
        self.split.set_sidebar_width_fraction(0.26);
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

        self.favourites_button.set_icon_name("starred-symbolic");
        self.favourites_button
            .set_tooltip_text(Some("Favourites and notes"));
        self.favourites_button.add_css_class("flat");
        self.favourites_button
            .set_action_name(Some("win.favourites"));
        header.pack_start(&self.favourites_button);

        self.help_button.set_icon_name("help-about-symbolic");
        self.help_button.set_tooltip_text(Some("About Roma (F1)"));
        self.help_button.add_css_class("flat");
        self.help_button.set_action_name(Some("win.help"));
        header.pack_end(&self.help_button);

        for button in [&self.previous, &self.next] {
            header.pack_end(button);
        }

        let me = self.clone();
        self.sidebar_toggle
            .connect_toggled(move |toggle| me.set_sidebar(toggle.is_active()));

        for (button, step, tip, action) in [
            (
                &self.previous,
                -1,
                "Previous chapter (Left)",
                "previous-chapter",
            ),
            (&self.next, 1, "Next chapter (Right)", "next-chapter"),
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
            .set_placeholder_text(Some("Search, or type John 3:16"));
        self.search_entry.add_css_class("search-field");
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

        self.search_entry.set_margin_top(10);
        self.search_entry.set_margin_bottom(8);
        self.search_entry.set_margin_start(12);
        self.search_entry.set_margin_end(12);
        sidebar.append(&self.search_entry);

        sidebar.append(&scroller(&self.books_list));

        sidebar.append(&gtk::Separator::new(gtk::Orientation::Horizontal));

        sidebar.append(&section_label("Chapters"));

        self.chips_box.set_selection_mode(gtk::SelectionMode::None);
        self.chips_box.set_homogeneous(false);
        self.chips_box.set_column_spacing(6);
        self.chips_box.set_row_spacing(6);
        // Kept tight, so a book with 150 chapters cannot stretch the sidebar.
        self.chips_box.set_min_children_per_line(3);
        self.chips_box.set_max_children_per_line(4);
        self.chips_box.set_margin_top(2);
        self.chips_box.set_margin_bottom(12);
        self.chips_box.set_margin_start(12);
        self.chips_box.set_margin_end(12);

        let chips = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Never)
            .vscrollbar_policy(gtk::PolicyType::Automatic)
            .child(&self.chips_box)
            .build();
        chips.set_vexpand(false);
        chips.add_css_class("chapters");
        // The scrollbar sits beside the chips rather than over them, so it can
        // never swallow a click on a chapter.
        chips.set_overlay_scrolling(false);
        chips.set_propagate_natural_height(true);
        chips.set_max_content_height(CHIPS_HEIGHT);
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

        self.favourites_heading.set_margin_top(12);
        self.favourites_heading.set_margin_bottom(4);
        self.favourites_heading.set_margin_start(16);
        self.favourites_heading.set_margin_end(16);
        self.favourites_heading.add_css_class("sidebar-heading");
        self.favourites_heading.set_xalign(0.0);

        self.favourites_empty
            .set_icon_name(Some("starred-symbolic"));
        self.favourites_empty.set_title("No favourites yet");
        self.favourites_empty.set_description(Some(
            "Star a verse while you read, and it waits for you here.",
        ));
        self.favourites_empty.set_vexpand(true);

        self.favourites_stack
            .add_named(&scroller(&self.favourites_list), Some("list"));
        self.favourites_stack
            .add_named(&self.favourites_empty, Some("empty"));
        self.favourites_stack.set_visible_child_name("list");
        self.favourites_stack.set_vexpand(true);

        let favourites = gtk::Box::new(gtk::Orientation::Vertical, 0);
        favourites.append(&self.favourites_heading);
        favourites.append(&self.favourites_stack);

        self.stack.add_named(&sidebar, Some("browser"));
        self.stack.add_named(&results, Some("results"));
        self.stack.add_named(&favourites, Some("favourites"));
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

        self.footer.add_css_class("document-footer");
        self.footer.set_margin_top(28);

        let document = gtk::Box::new(gtk::Orientation::Vertical, 0);
        document.set_margin_top(36);
        document.set_margin_bottom(36);
        document.set_margin_start(28);
        document.set_margin_end(28);
        document.append(&self.eyebrow);
        document.append(&self.title);
        document.append(&self.divider);
        document.append(&self.verses);
        document.append(&self.footer);
        document.append(&self.empty_page);

        let clamp = adw::Clamp::new();
        clamp.set_maximum_size(READING_WIDTH);
        clamp.set_tightening_threshold(READING_WIDTH - 80);
        clamp.set_child(Some(&document));

        self.scroller
            .set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
        self.scroller.set_vexpand(true);
        self.scroller.set_child(Some(&clamp));

        // Where a picked verse is kept or annotated: one star, bottom right.
        self.tools_reference.add_css_class("verse-tools-reference");
        self.tools_reference.set_xalign(0.0);
        self.tools_reference.set_margin_end(4);

        self.tools_star.add_css_class("verse-tools-button");
        self.tools_star.connect_clicked({
            let me = self.clone();
            move |_| me.toggle_favourite()
        });

        self.tools_note.set_icon_name("document-edit-symbolic");
        self.tools_note.add_css_class("verse-tools-button");
        self.tools_note
            .set_tooltip_text(Some("Write a note on this verse"));
        self.tools_note.connect_clicked({
            let me = self.clone();
            move |_| me.edit_note()
        });

        let tools = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        tools.add_css_class("verse-tools");
        tools.set_halign(gtk::Align::End);
        tools.set_margin_end(24);
        tools.set_margin_top(6);
        tools.set_margin_bottom(14);
        tools.append(&self.tools_reference);
        tools.append(&self.tools_note);
        tools.append(&self.tools_star);

        self.tools_revealer.set_child(Some(&tools));
        self.tools_revealer
            .set_transition_type(gtk::RevealerTransitionType::Crossfade);
        self.tools_revealer.set_transition_duration(120);
        self.tools_revealer.set_reveal_child(false);

        let reader = gtk::Box::new(gtk::Orientation::Vertical, 0);
        reader.append(&self.scroller);
        reader.append(&self.tools_revealer);
        reader.upcast()
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

        let favourites_factory = gtk::SignalListItemFactory::new();
        favourites_factory.connect_setup(|_, item| setup_result_row(item));
        let me = self.clone();
        favourites_factory.connect_bind(move |_, item| {
            let me = me.clone();
            let (reference, note) = {
                let rows = me.favourites_rows.borrow();
                let Some(row) = rows.get(item.position() as usize) else {
                    return;
                };
                let note = if row.note.is_empty() {
                    me.bible
                        .verse_text(me.bible.book_index(&row.book), row.chapter, row.verse)
                        .unwrap_or_default()
                        .to_string()
                } else {
                    row.note.clone()
                };
                (row.reference(), note)
            };
            let Some(row) = item.child().and_downcast::<gtk::Box>() else {
                return;
            };
            if let Some(label) = row.first_child().and_downcast::<gtk::Label>() {
                label.set_text(&reference);
            }
            if let Some(label) = row.last_child().and_downcast::<gtk::Label>() {
                label.set_text(&note);
            }
        });
        self.favourites_list.set_factory(Some(&favourites_factory));

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
                set_markup(&label, &snippet);
            }
        });

        self.books_selection.set_model(Some(&self.books_model));
        self.books_list.set_model(Some(&self.books_selection));
        self.books_list.set_factory(Some(&books_factory));

        self.results_selection.set_model(Some(&self.results_model));
        self.results_selection.set_can_unselect(false);
        self.results_list.set_model(Some(&self.results_selection));
        self.results_list.set_factory(Some(&results_factory));

        self.favourites_selection
            .set_model(Some(&self.favourites_model));
        self.favourites_selection.set_can_unselect(false);
        self.favourites_list
            .set_model(Some(&self.favourites_selection));
        self.favourites_list.set_factory(Some(&favourites_factory));
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
        self.favourites_selection
            .connect_selected_notify(move |selection| {
                let me = me.clone();
                me.on_favourite_selected(selection.selected());
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
                gtk::gdk::Key::Escape => me.select_verse(NO_VERSE),
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
            ("help", ["F1", "<Control>question"].as_slice()),
            ("favourites", ["<Control>d"].as_slice()),
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
            "help" => self.show_help(),
            "favourites" => self.show_favourites(),
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

    /// The saved favourites, for callers that want to read or write them.
    pub fn favourites(&self) -> &store::Favourites {
        &self.favourites
    }

    /// Draws the chapter that is open again, as if it had just been opened.
    pub fn reopen(self: &Rc<Self>) {
        self.render_chapter(self.book.get(), self.chapter.get());
    }

    /// The help window, built once and shown again on every visit.
    fn show_help(self: &Rc<Self>) {
        if self.help_window.borrow().is_none() {
            let window = crate::help::build(&self.window);
            window.connect_close_request({
                let me = self.clone();
                move |_| {
                    *me.help_window.borrow_mut() = None;
                    glib::Propagation::Proceed
                }
            });
            *self.help_window.borrow_mut() = Some(window);
        }
        if let Some(window) = self.help_window.borrow().as_ref() {
            window.present();
        }
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

    fn render_chapter(self: &Rc<Self>, book: u32, chapter: u32) {
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
        let verses = self
            .bible
            .chapter_verses(book, chapter)
            .iter()
            .filter(|verse| !verse.text.is_empty())
            .collect::<Vec<_>>();
        let texts = verses
            .iter()
            .map(|verse| verse.text.as_str())
            .collect::<Vec<_>>();
        let speaking = jesus_speaks(&texts, self.is_new_testament(book));
        let book_name = self
            .bible
            .book(book)
            .map_or("", |book| book.name.as_str())
            .to_string();
        for (verse, speaks) in verses.iter().zip(speaking) {
            self.verses
                .append(&self.verse_row(verse, speaks, &book_name, chapter));
            shown += 1;
        }

        self.empty_page.set_visible(shown == 0);
        self.divider.set_visible(shown > 0);
        self.footer
            .set_text(&format!("{shown} verse{} \u{b7} {section}", plural(shown)));
        self.update_picked();
    }

    fn verse_row(
        self: &Rc<Self>,
        verse: &Verse,
        speaks: bool,
        book_name: &str,
        chapter: u32,
    ) -> gtk::Box {
        let row = gtk::Box::new(gtk::Orientation::Horizontal, 16);
        row.add_css_class("verse");

        let label = gtk::Label::new(Some(&verse.number.to_string()));
        label.add_css_class("verse-number");
        label.set_xalign(1.0);
        label.set_width_chars(3);

        let text = gtk::Label::new(None);
        set_markup(&text, &verse.text);
        text.add_css_class("verse-text");
        // The words of Jesus, in the red of the stylesheet.
        if speaks {
            text.add_css_class("speaks");
        }
        text.set_wrap(true);
        text.set_xalign(0.0);
        text.set_selectable(true);

        // The words, and the note underneath them when there is one.
        let column = gtk::Box::new(gtk::Orientation::Vertical, 4);
        column.set_hexpand(true);
        column.append(&text);

        let number = verse.number;
        let starred = self.favourites.has(book_name, chapter, number);
        if starred {
            row.add_css_class("favourite");
            if let Some(note) = self.favourites.note(book_name, chapter, number) {
                let note = gtk::Label::new(Some(&note));
                note.add_css_class("verse-note-text");
                note.set_xalign(0.0);
                note.set_wrap(true);
                note.set_selectable(true);
                column.append(&note);
            }
        }

        // A click picks the verse; the tools come up at the bottom of the pane.
        // Dragging across the text is left alone, so words can still be copied.
        let click = gtk::GestureClick::new();
        click.set_button(gtk::gdk::BUTTON_PRIMARY);
        let pressed_at = Rc::new(Cell::new((0.0f64, 0.0f64)));
        let origin = pressed_at.clone();
        click.connect_pressed(move |_, _, x, y| origin.set((x, y)));
        let me = self.clone();
        click.connect_released(move |_, presses, x, y| {
            let (start_x, start_y) = pressed_at.get();
            let dragged = (x - start_x).abs() > 4.0 || (y - start_y).abs() > 4.0;
            if presses == 1 && !dragged {
                me.select_verse(number);
            }
        });
        row.add_controller(click);

        row.append(&label);
        row.append(&column);
        self.verse_rows
            .borrow_mut()
            .insert(verse.number, row.clone());
        row
    }

    // ------------------------------------------------------------ favourites

    /// Picks a verse, or puts it down again if it was already picked. The
    /// tools come up at the bottom right while a verse is picked.
    pub fn select_verse(self: &Rc<Self>, verse: u32) {
        let picked = if self.selected_verse.get() == verse {
            NO_VERSE
        } else {
            verse
        };
        self.selected_verse.set(picked);
        self.update_picked();
    }

    /// The verse that is picked, with the book and chapter it sits in.
    fn picked(&self) -> Option<(u32, u32, u32)> {
        let verse = self.selected_verse.get();
        if verse == NO_VERSE || !self.verse_rows.borrow().contains_key(&verse) {
            return None;
        }
        Some((self.book.get(), self.chapter.get(), verse))
    }

    /// Marks the picked row, and fills the tools: they follow every redraw.
    fn update_picked(&self) {
        let rows = self.verse_rows.borrow();
        for (number, row) in rows.iter() {
            if *number == self.selected_verse.get() {
                row.add_css_class("selected");
            } else {
                row.remove_css_class("selected");
            }
        }
        drop(rows);

        let Some((book, chapter, verse)) = self.picked() else {
            self.tools_revealer.set_reveal_child(false);
            return;
        };
        let name = self
            .bible
            .book(book)
            .map_or("", |book| book.name.as_str())
            .to_string();
        self.tools_reference
            .set_text(&format!("{name} {chapter}:{verse}"));
        let starred = self.favourites.has(&name, chapter, verse);
        self.tools_star.set_icon_name(if starred {
            "starred-symbolic"
        } else {
            "non-starred-symbolic"
        });
        self.tools_star.set_tooltip_text(Some(if starred {
            "Remove from favourites"
        } else {
            "Keep this verse"
        }));
        // A note only means something on a verse that is kept.
        self.tools_note.set_visible(starred);
        self.tools_revealer.set_reveal_child(true);
    }

    /// Keeps or drops the picked verse, and redraws the chapter and the list.
    fn toggle_favourite(self: &Rc<Self>) {
        let Some((book, chapter, verse)) = self.picked() else {
            return;
        };
        let name = self
            .bible
            .book(book)
            .map_or("", |book| book.name.as_str())
            .to_string();
        let starred = self.favourites.toggle(&name, chapter, verse);
        let reference = format!("{name} {chapter}:{verse}");
        self.notify(if starred {
            format!("Kept {reference}")
        } else {
            format!("Removed {reference}")
        });
        self.refresh_favourites();
        self.render_chapter(self.book.get(), self.chapter.get());
    }

    /// The favourites page, with the books out beside it.
    fn show_favourites(self: &Rc<Self>) {
        self.set_sidebar(true);
        self.refresh_favourites();
        if self.stack.visible_child_name().as_deref() != Some("favourites") {
            self.stack.set_visible_child_name("favourites");
        }
    }

    /// Rebuilds the favourites list from the file.
    fn refresh_favourites(&self) {
        let rows = self.favourites.all();
        self.favourites_model.remove_all();
        for row in &rows {
            self.favourites_model
                .append(&gtk::StringObject::new(&row.reference()));
        }
        *self.favourites_rows.borrow_mut() = rows;
        self.favourites_heading
            .set_text(&match self.favourites.len() {
                0 => "Favourites".to_string(),
                1 => "Favourites \u{b7} 1 verse".to_string(),
                count => format!("Favourites \u{b7} {count} verses"),
            });
        self.favourites_stack
            .set_visible_child_name(if self.favourites.is_empty() {
                "empty"
            } else {
                "list"
            });
    }

    fn on_favourite_selected(self: &Rc<Self>, position: u32) {
        if self.syncing.get() {
            return;
        }
        let row = self
            .favourites_rows
            .borrow()
            .get(position as usize)
            .cloned();
        let Some(row) = row else {
            return;
        };
        let book = self.bible.book_index(&row.book);
        if book == u32::MAX {
            self.notify(format!("{} is not in this Bible", row.book));
            return;
        }
        self.go_to(book, row.chapter, Some(row.verse));
        self.scroll_list_to(&self.favourites_list, position);
    }

    /// A small window for the note on the picked verse.
    fn edit_note(self: &Rc<Self>) {
        let Some((book, chapter, verse)) = self.picked() else {
            return;
        };
        let book = self
            .bible
            .book(book)
            .map_or("", |book| book.name.as_str())
            .to_string();
        let reference = format!("{book} {chapter}:{verse}");
        let existing = self
            .favourites
            .note(&book, chapter, verse)
            .unwrap_or_default();

        let window = adw::Window::builder()
            .title(format!("Note on {reference}"))
            .transient_for(&self.window)
            .modal(true)
            .default_width(420)
            .default_height(320)
            .build();

        let header = adw::HeaderBar::new();
        let title = adw::WindowTitle::new(&reference, "Note");
        header.set_title_widget(Some(&title));

        let cancel = gtk::Button::with_label("Cancel");
        cancel.add_css_class("flat");
        let closing = window.clone();
        cancel.connect_clicked(move |_| closing.close());
        header.pack_start(&cancel);

        let view = gtk::TextView::new();
        view.set_wrap_mode(gtk::WrapMode::WordChar);
        view.set_top_margin(10);
        view.set_bottom_margin(10);
        view.set_left_margin(10);
        view.set_right_margin(10);
        view.set_accepts_tab(false);
        view.add_css_class("note-view");

        let buffer = view.buffer();
        buffer.set_text(&existing);

        let save = gtk::Button::with_label("Save");
        save.add_css_class("suggested-action");
        let me = self.clone();
        let book = book.to_string();
        let saving = window.clone();
        save.connect_clicked(move |_| {
            let me = me.clone();
            let book = book.clone();
            let note = buffer.text(&buffer.start_iter(), &buffer.end_iter(), false);
            me.favourites.set_note(&book, chapter, verse, note.as_str());
            me.refresh_favourites();
            me.render_chapter(me.book.get(), me.chapter.get());
            if note.trim().is_empty() {
                me.notify(format!("Note cleared on {reference}"));
            } else {
                me.notify(format!("Note saved on {reference}"));
            }
            saving.close();
        });
        header.pack_end(&save);

        let scroller = gtk::ScrolledWindow::builder()
            .vexpand(true)
            .child(&view)
            .build();
        let toolbar = adw::ToolbarView::new();
        toolbar.add_top_bar(&header);
        toolbar.set_content(Some(&scroller));
        window.set_content(Some(&toolbar));
        window.present();
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
        // Favourites asked to be seen, so leave it on show.
        if self.stack.visible_child_name().as_deref() != Some("favourites") {
            self.stack.set_visible_child_name("browser");
        }
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

/// Shows a label's text with every mention of Jesus in red and bold.
fn set_markup(label: &gtk::Label, text: &str) {
    label.set_markup(&markup(text));
}

/// Marks up `text`, colouring every mention of Jesus.
fn markup(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len() + 32);
    let mut plain = String::new();
    let mut index = 0;

    while index < chars.len() {
        let start = index;
        let end = index + NAME.len();
        let said = end <= chars.len()
            && chars[start..end]
                .iter()
                .collect::<String>()
                .eq_ignore_ascii_case(NAME)
            && (start == 0 || !chars[start - 1].is_alphanumeric())
            && (end == chars.len() || !chars[end].is_alphanumeric());
        if said {
            if !plain.is_empty() {
                out.push_str(&glib::markup_escape_text(&plain));
                plain.clear();
            }
            out.push_str(&format!(
                "<span foreground=\"{ACCENT}\" weight=\"bold\">{NAME}</span>"
            ));
            index = end;
        } else {
            plain.push(chars[index]);
            index += 1;
        }
    }

    if !plain.is_empty() {
        out.push_str(&glib::markup_escape_text(&plain));
    }
    out
}

// ------------------------------------------------------- who is speaking

/// Verbs that introduce what somebody says.
const SPEAKS: &[&str] = &[
    "said",
    "says",
    "say",
    "answered",
    "responded",
    "replied",
    "spoke",
    "cried",
    "asked",
    "told",
    "preached",
    "proclaimed",
    "commanded",
    "instructed",
    "explained",
    "exclaimed",
    "taught",
];

/// Words Jesus uses of himself, where no verb names him as the speaker.
const FIRST_PERSON: &[&[&str]] = &[
    &["i", "say", "to", "you"],
    &["i", "say", "unto", "you"],
    &["i", "tell", "you"],
    &["i", "have", "said"],
    &["i", "have", "spoken"],
    &["i", "give", "you"],
    &["amen", "amen"],
    &["i", "am", "the", "way"],
    &["i", "am", "the", "bread"],
    &["i", "am", "the", "light"],
    &["i", "have", "come"],
];

/// Words between the speaker and their verb, such as "and the Lord of hosts".
const CONNECTIVES: &[&str] = &[
    "and", "the", "a", "an", "of", "to", "with", "then", "but", "so", "that", "which", "who",
    "himself", "him", "them", "unto", "upon", "as", "at", "in", "for", "by", "from", "it", "is",
    "was", "were", "there", "also", "even", "thus", "while", "when", "truly", "if",
];

/// True for each verse in a chapter: is this the speech of Jesus?
///
/// The text carries no speaker of its own, so the verses are read in order and
/// followed: a verb naming Jesus, or the first person of the New Testament,
/// begins his speech; a quotation he opened stays his until it is closed; and
/// anyone else taking the floor, or a book before the New Testament, ends it.
fn jesus_speaks(chapter: &[&str], new_testament: bool) -> Vec<bool> {
    let mut flags = Vec::with_capacity(chapter.len());
    let mut speaking = false;
    let mut quoted = false;

    for text in chapter {
        let words = words_of(text);
        let subject = speaker_of(&words);
        let his = match subject {
            Some(subject) => is_him(subject, new_testament) && !speaks_to_him(&words),
            None => false,
        };
        let cue = his || says_of_himself(&words);
        let hands_over =
            subject.is_some_and(|subject| !is_him(subject, new_testament)) && speaks(&words);
        let opens = text.matches('\u{201c}').count();
        let closes = text.matches('\u{201d}').count();
        let ends = closes > opens;

        let here = cue || quoted || new_testament && speaking && subject.is_none();
        if opens > closes {
            quoted = cue || new_testament && speaking && !hands_over;
        } else if ends {
            quoted = false;
        }
        speaking = here && !hands_over && !ends;

        flags.push(here);
    }

    flags
}

/// Splits a verse into lowercased words, dropping punctuation and quotes.
fn words_of(text: &str) -> Vec<String> {
    text.split(|c: char| !c.is_alphanumeric())
        .filter(|word| !word.is_empty())
        .map(str::to_lowercase)
        .collect()
}

/// True when the verse carries a verb of speech.
fn speaks(words: &[String]) -> bool {
    words.iter().any(|word| SPEAKS.contains(&word.as_str()))
}

/// True when Jesus is named in the verse.
fn names_him(words: &[String]) -> bool {
    words.iter().any(|word| word == NAME || word == "christ")
}

/// The word in front of the first verb of speech: who the words belong to.
fn speaker_of(words: &[String]) -> Option<&str> {
    let verb = words
        .iter()
        .position(|word| SPEAKS.contains(&word.as_str()))?;
    words[..verb]
        .iter()
        .rev()
        .find(|word| !CONNECTIVES.contains(&word.as_str()))
        .map(String::as_str)
}

/// True when `subject` is Jesus: his name, or the narrator's "he" and "I".
fn is_him(subject: &str, new_testament: bool) -> bool {
    matches!(subject, NAME | "christ") || new_testament && matches!(subject, "he" | "i")
}

/// True when the words are addressed to Jesus, so the speaker is someone else.
fn speaks_to_him(words: &[String]) -> bool {
    names_him(words)
        && words
            .windows(2)
            .any(|pair| pair == ["to", "him"] || pair == ["to", "them"])
}

/// True when Jesus speaks of himself: "Amen, amen, I say to you".
fn says_of_himself(words: &[String]) -> bool {
    FIRST_PERSON.iter().any(|phrase| {
        words
            .windows(phrase.len())
            .any(|window| window.iter().map(String::as_str).eq(phrase.iter().copied()))
    })
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

#[cfg(test)]
mod tests {
    use super::{jesus_speaks, markup};

    const RED: &str = "<span foreground=\"#ff4d45\" weight=\"bold\">Jesus</span>";

    #[test]
    fn marks_the_name_wherever_it_is_said() {
        assert_eq!(markup("Jesus wept."), format!("{RED} wept."));
        assert_eq!(
            markup("They saw Jesus, and feared."),
            format!("They saw {RED}, and feared.")
        );
        assert_eq!(markup("of jesus"), format!("of {RED}"));
        assert_eq!(markup("JESUS"), RED);
    }

    #[test]
    fn leaves_other_uses_of_the_letters_alone() {
        assert_eq!(markup("Jesusalpha, jesusling"), "Jesusalpha, jesusling");
    }

    #[test]
    fn escapes_the_rest_of_the_text() {
        assert_eq!(
            markup("Jesus said: \"God & Son\""),
            format!("{RED} said: &quot;God &amp; Son&quot;")
        );
    }

    fn speaks_of(lines: &[&str], new_testament: bool) -> Vec<bool> {
        jesus_speaks(lines, new_testament)
    }

    #[test]
    fn reds_the_verses_where_jesus_speaks() {
        assert_eq!(
            speaks_of(
                &[
                    "Then there was a man among the Pharisees, named Nicodemus.",
                    "Jesus responded and said to him, \u{201c}Amen, amen, I say to you.\u{201d}",
                    "Nicodemus said to him: \u{201c}How can this be?\u{201d}",
                ],
                true,
            ),
            vec![false, true, false]
        );
    }

    #[test]
    fn follows_a_quotation_until_it_is_closed() {
        assert_eq!(
            speaks_of(
                &[
                    "And opening his mouth, he taught them, saying:",
                    "\u{201c}Blessed are the poor in spirit, for theirs is the kingdom.",
                    "Blessed are the meek, for they shall possess the earth.\u{201d}",
                    "And he went up the mountain.",
                ],
                true,
            ),
            vec![true, true, true, false]
        );
    }

    #[test]
    fn leaves_others_to_their_own_words() {
        assert_eq!(
            speaks_of(
                &[
                    "And the Lord answered the angel, who had been speaking with me.",
                    "And he said to me: Cry out, saying: Thus says the Lord of hosts.",
                ],
                false,
            ),
            vec![false, false]
        );
    }
}
