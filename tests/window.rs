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

//! Widget level checks for the reader, run against the real bible text.
//!
//! These need a display; without one they are skipped.

use std::path::PathBuf;
use std::rc::Rc;

use gtk::Application;
use gtk::prelude::*;
use libadwaita as adw;

use roma::bible::Bible;
use roma::ui::{MainWindow, STYLE};

fn bible() -> Rc<Bible> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(roma::BIBLE_FILE);
    Rc::new(Bible::load(&path).expect("bible text loads"))
}

/// The reader window, or `None` when there is no display to build it on.
fn reader() -> Option<Rc<MainWindow>> {
    adw::init().ok()?;
    let app = Application::builder()
        .application_id("dev.roma.ReaderTest")
        .build();
    Some(MainWindow::new(&app, bible()))
}

fn descendants(widget: &impl IsA<gtk::Widget>) -> Vec<gtk::Widget> {
    let mut found = Vec::new();
    let mut child = widget.first_child();
    while let Some(widget) = child {
        found.push(widget.clone());
        found.extend(descendants(&widget));
        child = widget.next_sibling();
    }
    found
}

fn has_class(widget: &gtk::Widget, class: &str) -> bool {
    widget.css_classes().iter().any(|name| name == class)
}

fn count(widgets: &[gtk::Widget], class: &str) -> usize {
    widgets
        .iter()
        .filter(|widget| has_class(widget, class))
        .count()
}

fn labels(widgets: &[gtk::Widget]) -> Vec<String> {
    widgets
        .iter()
        .filter_map(|widget| widget.downcast_ref::<gtk::Label>())
        .map(|label| label.text().to_string())
        .collect()
}

fn opens_on_the_first_chapter() {
    let Some(reader) = reader() else {
        return;
    };
    assert_eq!(reader.chapter_title(), "Genesis 1");
    assert_eq!(reader.shown_verses(), 31);

    // The footer carries the way through the bible.
    let widgets = descendants(reader.window());
    assert_eq!(count(&widgets, "chapter-button"), 2, "previous and next");
    let footer: Vec<&gtk::Widget> = widgets
        .iter()
        .filter(|widget| has_class(widget, "document-footer"))
        .collect();
    assert_eq!(footer.len(), 1);
    assert_eq!(footer[0].downcast_ref::<gtk::Label>().unwrap().text(), "Genesis 1 \u{b7} 31 verses");
}

fn titles_are_headings_and_verses_are_bold_paragraphs() {
    let Some(reader) = reader() else {
        return;
    };
    let widgets = descendants(reader.window());

    let titles: Vec<&gtk::Widget> = widgets
        .iter()
        .filter(|widget| has_class(widget, "title-1"))
        .collect();
    assert_eq!(titles.len(), 1, "one heading per chapter");
    assert_eq!(
        titles[0].downcast_ref::<gtk::Label>().unwrap().text(),
        "Genesis 1"
    );

    let verses: Vec<&gtk::Widget> = widgets
        .iter()
        .filter(|widget| has_class(widget, "verse"))
        .collect();
    assert_eq!(verses.len(), 31, "one paragraph per verse");
    for verse in &verses {
        let parts = descendants(*verse);
        let numbers: Vec<&gtk::Label> = parts
            .iter()
            .filter(|part| has_class(part, "verse-number"))
            .filter_map(|part| part.downcast_ref::<gtk::Label>())
            .collect();
        let texts: Vec<&gtk::Label> = parts
            .iter()
            .filter(|part| has_class(part, "verse-text"))
            .filter_map(|part| part.downcast_ref::<gtk::Label>())
            .collect();
        assert_eq!(numbers.len(), 1);
        assert_eq!(texts.len(), 1);
        assert!(numbers[0].text().as_str().parse::<u32>().is_ok());
        assert!(!texts[0].text().is_empty());
    }

    // The heading and the verse paragraphs are styled by the stylesheet.
    assert!(STYLE.contains(".title-1 {"));
    assert!(STYLE.contains(".verse-text {"));
    assert!(STYLE.contains("font-weight: bold"));
}

fn lists_every_book_and_the_chapters_of_the_current_one() {
    let Some(reader) = reader() else {
        return;
    };
    let widgets = descendants(reader.window());
    let bible = bible();

    let books = count(&widgets, "book");
    assert_eq!(books, bible.book_count() as usize, "a row per book");
    assert!(
        count(&widgets, "book-section") >= 2,
        "the books are grouped by testament"
    );

    let chapters = bible
        .book(0)
        .map(|book| book.chapters.len())
        .unwrap_or_default();
    assert_eq!(count(&widgets, "chip"), chapters, "a chip per chapter");

    // The chips are laid out horizontally, in a scroller, and one is selected.
    let selected = widgets
        .iter()
        .filter(|widget| has_class(widget, "chip"))
        .filter(|widget| {
            widget
                .downcast_ref::<gtk::ToggleButton>()
                .is_some_and(|chip| chip.is_active())
        })
        .count();
    assert_eq!(selected, 1, "the open chapter is the selected chip");
}

fn jumps_to_a_passage_from_a_reference() {
    let Some(reader) = reader() else {
        return;
    };
    reader.jump("John 3:16");
    assert_eq!(reader.chapter_title(), "John 3");
    assert_eq!(reader.shown_verses(), 36);
    assert_eq!(reader.highlighted_verse(), Some(16));

    reader.jump("ps 23");
    assert_eq!(reader.chapter_title(), "Psalms 23");
    assert_eq!(reader.highlighted_verse(), None);

    reader.jump("2 tim 3:16-17");
    assert_eq!(reader.chapter_title(), "II Timothy 3");
}

fn searching_words_lists_matches() {
    let Some(reader) = reader() else {
        return;
    };
    reader.search("green pastures");
    let results = reader.results();
    assert_eq!(results.len(), 1, "one match");
    assert!(results.iter().all(|hit| !hit.snippet.is_empty()));
    assert_eq!(
        results
            .first()
            .map(|hit| (hit.book, hit.chapter, hit.verse)),
        Some((30, 34, 14)),
        "Ezekiel 34:14"
    );

    let widgets = descendants(reader.window());
    let texts = labels(&widgets);
    assert!(
        texts.iter().any(|text| text == "Ezekiel 34:14"),
        "matches are labelled with their reference"
    );

    reader.search("green");
    assert!(reader.results().len() > 10, "more matches for one word");

    reader.search("shepherd");
    assert!(reader.results().len() > 50, "many matches");

    reader.search("qqzzxx-nothing-matches-this");
    assert!(reader.results().is_empty());
    let texts = labels(&descendants(reader.window()));
    assert!(
        texts.iter().any(|text| text == "No results"),
        "an empty state is offered"
    );

    let opened = reader.chapter_title();
    reader.clear_search();
    assert!(reader.results().is_empty());
    assert_eq!(reader.chapter_title(), opened, "the reading pane is kept");
}

fn walks_forward_and_back_through_chapters() {
    let Some(reader) = reader() else {
        return;
    };
    reader.go_to(49, 1, None);
    assert_eq!(reader.chapter_title(), "John 1");
    reader.go_to(49, 3, None);
    assert_eq!(reader.chapter_title(), "John 3");
    // Missing chapters are ignored rather than shown.
    reader.go_to(49, 99, None);
    assert_eq!(reader.chapter_title(), "John 3");
    reader.go_to(500, 1, None);
    assert_eq!(reader.chapter_title(), "John 3");
}

fn chapters_without_text_say_so() {
    let Some(reader) = reader() else {
        return;
    };
    // "Prayer of Manasses" has no text in this translation.
    reader.go_to(73, 1, None);
    assert_eq!(reader.shown_verses(), 0);

    let texts = labels(&descendants(reader.window()));
    assert!(
        texts.iter().any(|text| text.contains("no text")),
        "the empty passage is explained"
    );
}

/// GTK belongs to the thread that started it, so every check shares one test.
#[test]
fn reader_window() {
    let checks: [(&str, fn()); 7] = [
        ("opens on the first chapter", opens_on_the_first_chapter),
        (
            "titles are headings and verses are bold paragraphs",
            titles_are_headings_and_verses_are_bold_paragraphs,
        ),
        (
            "lists every book and the chapters of the current one",
            lists_every_book_and_the_chapters_of_the_current_one,
        ),
        (
            "jumps to a passage from a reference",
            jumps_to_a_passage_from_a_reference,
        ),
        (
            "searching words lists matches",
            searching_words_lists_matches,
        ),
        (
            "walks forward and back through chapters",
            walks_forward_and_back_through_chapters,
        ),
        ("chapters without text say so", chapters_without_text_say_so),
    ];

    if gtk::init().is_err() {
        println!("no display: skipped");
        return;
    }
    for (name, check) in checks {
        print!("{name} ... ");
        check();
        println!("ok");
    }
}
