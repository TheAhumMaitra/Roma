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

//! Loading and querying the Bible text.
//!
//! The source JSON (`sources/CPDV.json`) is a nested list of
//! books -> chapters -> verses, so it is flattened into a single verse array on
//! load: books and chapters keep the indices of their verses, which makes both
//! rendering a chapter and scanning the whole text cheap.

use std::error::Error;
use std::fs::File;
use std::io::{BufReader, Error as IoError, ErrorKind};
use std::path::Path;

use serde::Deserialize;

/// Maximum number of hits kept in one search result set.
pub const MAX_HITS: usize = 300;

const SECTION_OT: &str = "Old Testament";
const SECTION_DEUTEROCANONICAL: &str = "Deuterocanonical";
const SECTION_NT: &str = "New Testament";
const SECTION_APOCRYPHA: &str = "Apocrypha";

#[derive(Deserialize)]
struct RawBible {
    books: Vec<RawBook>,
}

#[derive(Deserialize)]
struct RawBook {
    name: String,
    chapters: Vec<RawChapter>,
}

#[derive(Deserialize)]
struct RawChapter {
    chapter: u32,
    #[serde(default)]
    verses: Vec<RawVerse>,
}

#[derive(Deserialize)]
struct RawVerse {
    verse: u32,
    #[serde(default)]
    text: String,
}

pub struct Verse {
    pub number: u32,
    pub text: String,
    lowered: String,
}

pub struct Chapter {
    pub number: u32,
    start: usize,
    end: usize,
}

impl Chapter {
    pub fn verse_count(&self) -> usize {
        self.end - self.start
    }
}

pub struct Book {
    pub name: String,
    pub section: &'static str,
    pub chapters: Vec<Chapter>,
    first: usize,
    aliases: Vec<String>,
}

impl Book {
    /// True in the New Testament, the only part where Jesus speaks in the text.
    pub fn is_new_testament(&self) -> bool {
        self.section == SECTION_NT
    }

    pub fn verse_count(&self) -> usize {
        self.chapters.last().map_or(0, |c| c.end - self.first)
    }

    pub fn chapter(&self, number: u32) -> Option<&Chapter> {
        self.chapters.iter().find(|c| c.number == number)
    }
}

pub struct Bible {
    books: Vec<Book>,
    verses: Vec<Verse>,
    /// Book and chapter each verse belongs to, parallel to `verses`.
    locations: Vec<(u32, u32)>,
}

/// A single search or jump result.
#[derive(Clone)]
pub struct Hit {
    pub book: u32,
    pub chapter: u32,
    pub verse: u32,
    pub snippet: String,
}

pub struct Results {
    pub hits: Vec<Hit>,
    /// Number of matches found, which may exceed `hits.len()`.
    pub total: usize,
    /// True when the query was a passage reference rather than free text.
    pub reference: bool,
}

/// A parsed user query: either a passage (`John 3:16`) or words to look for.
#[derive(Debug)]
pub enum Query {
    Passage {
        book: u32,
        chapters: Vec<u32>,
        verses: Option<Vec<u32>>,
    },
    Text(Vec<String>),
}

impl Bible {
    pub fn load(path: &Path) -> Result<Self, Box<dyn Error>> {
        let file = File::open(path).map_err(|err| {
            IoError::new(
                err.kind(),
                format!("cannot open bible text {}: {err}", path.display()),
            )
        })?;
        let bible = Self::from_reader(BufReader::new(file))?;
        if bible.books.is_empty() {
            return Err(Box::new(IoError::new(
                ErrorKind::InvalidData,
                format!("{} contains no books", path.display()),
            )));
        }
        Ok(bible)
    }

    /// Reads the bible text from JSON, in the shape of `sources/CPDV.json`.
    pub fn from_reader<R: std::io::Read>(reader: R) -> Result<Self, serde_json::Error> {
        Ok(Self::from_raw(serde_json::from_reader(reader)?))
    }

    fn from_raw(raw: RawBible) -> Self {
        let names: Vec<&str> = raw.books.iter().map(|b| b.name.as_str()).collect();
        let sections = section_labels(&names);

        let mut verses = Vec::new();
        let mut locations = Vec::new();
        let mut books = Vec::with_capacity(raw.books.len());

        for (book_index, raw_book) in raw.books.into_iter().enumerate() {
            let first = verses.len();
            let mut chapters = Vec::with_capacity(raw_book.chapters.len());

            for raw_chapter in raw_book.chapters {
                let start = verses.len();
                for raw_verse in raw_chapter.verses {
                    verses.push(Verse {
                        number: raw_verse.verse,
                        lowered: raw_verse.text.to_lowercase(),
                        text: raw_verse.text,
                    });
                    locations.push((book_index as u32, raw_chapter.chapter));
                }
                chapters.push(Chapter {
                    number: raw_chapter.chapter,
                    start,
                    end: verses.len(),
                });
            }

            books.push(Book {
                aliases: aliases_for(&raw_book.name),
                name: raw_book.name,
                section: sections[book_index],
                chapters,
                first,
            });
        }

        Bible {
            books,
            verses,
            locations,
        }
    }

    pub fn books(&self) -> &[Book] {
        &self.books
    }

    pub fn book_count(&self) -> u32 {
        self.books.len() as u32
    }

    pub fn book(&self, index: u32) -> Option<&Book> {
        self.books.get(index as usize)
    }

    pub fn verse_count(&self) -> usize {
        self.verses.len()
    }

    pub fn chapter_verses(&self, book: u32, chapter: u32) -> &[Verse] {
        match self.books.get(book as usize).and_then(|b| b.chapter(chapter)) {
            Some(chapter) => &self.verses[chapter.start..chapter.end],
            None => &[],
        }
    }

    pub fn chapter_has_text(&self, book: u32, chapter: u32) -> bool {
        self.chapter_verses(book, chapter)
            .iter()
            .any(|v| !v.text.is_empty())
    }

    pub fn reference(&self, book: u32, chapter: u32, verse: u32) -> String {
        match self.books.get(book as usize) {
            Some(b) => format!("{} {}:{}", b.name, chapter, verse),
            None => format!("{chapter}:{verse}"),
        }
    }

    pub fn chapter_reference(&self, book: u32, chapter: u32) -> String {
        match self.books.get(book as usize) {
            Some(b) => format!("{} {}", b.name, chapter),
            None => chapter.to_string(),
        }
    }

    /// The next chapter after `book`/`chapter`, wrapping into the following book.
    pub fn next_chapter(&self, book: u32, chapter: u32) -> Option<(u32, u32)> {
        if let Some(next) = self
            .books
            .get(book as usize)
            .and_then(|b| b.chapters.iter().find(|c| c.number > chapter))
        {
            return Some((book, next.number));
        }
        let following = self.books.get(book as usize + 1)?;
        Some((book + 1, following.chapters.first()?.number))
    }

    /// The chapter before `book`/`chapter`, wrapping back into the previous book.
    pub fn previous_chapter(&self, book: u32, chapter: u32) -> Option<(u32, u32)> {
        if let Some(previous) = self
            .books
            .get(book as usize)
            .and_then(|b| b.chapters.iter().rev().find(|c| c.number < chapter))
        {
            return Some((book, previous.number));
        }
        let preceding = self.books.get(book.checked_sub(1)? as usize)?;
        Some((book - 1, preceding.chapters.last()?.number))
    }

    /// Resolves a book name, honouring abbreviations (`gen`, `ps`, `1 john`).
    pub fn find_book(&self, query: &str) -> Option<u32> {
        for candidate in book_candidates(query) {
            let needle = normalize(&candidate);
            if needle.is_empty() {
                continue;
            }
            if let Some(index) = self
                .books
                .iter()
                .position(|b| b.aliases.iter().any(|a| a == &needle))
            {
                return Some(index as u32);
            }
            let longest = self
                .books
                .iter()
                .enumerate()
                .filter_map(|(index, book)| {
                    book.aliases
                        .iter()
                        .filter(|a| a.len() >= 2 && a.starts_with(&needle))
                        .map(|a| a.len())
                        .max()
                        .map(|len| (index, len))
                })
                .max_by_key(|(_, len)| *len);
            if let Some((index, _)) = longest {
                return Some(index as u32);
            }
        }
        None
    }

    /// Parses a query as a passage reference when possible, otherwise as words.
    pub fn parse_query(&self, input: &str) -> Option<Query> {
        let input = input.trim();
        if input.is_empty() {
            return None;
        }

        let words: Vec<&str> = input.split_whitespace().collect();
        if let Some(position) = words
            .iter()
            .rposition(|w| is_number_spec(w))
            .filter(|position| *position > 0)
        {
            if let Some(book) = self.find_book(&words[..position].join(" ")) {
                let (chapters, verses) = parse_numbers(words[position]);
                if !chapters.is_empty() {
                    return Some(Query::Passage {
                        book,
                        chapters,
                        verses,
                    });
                }
            }
        }

        let terms: Vec<String> = input
            .split(|c: char| !c.is_alphanumeric())
            .filter(|w| !w.is_empty())
            .map(|w| w.to_lowercase())
            .collect();
        if terms.is_empty() {
            return None;
        }
        Some(Query::Text(terms))
    }

    pub fn search(&self, query: &Query) -> Results {
        match query {
            Query::Passage {
                book,
                chapters,
                verses,
            } => self.search_passage(*book, chapters, verses.as_deref()),
            Query::Text(words) => self.search_words(words),
        }
    }

    fn search_passage(&self, book: u32, chapters: &[u32], verses: Option<&[u32]>) -> Results {
        let mut hits = Vec::new();
        for &chapter in chapters {
            let chapter_verses = self.chapter_verses(book, chapter);
            let selected: Vec<&Verse> = match verses {
                Some(numbers) => numbers
                    .iter()
                    .filter_map(|number| chapter_verses.iter().find(|v| v.number == *number))
                    .collect(),
                None => chapter_verses.iter().collect(),
            };
            for verse in selected {
                hits.push(Hit {
                    book,
                    chapter,
                    verse: verse.number,
                    snippet: verse.text.clone(),
                });
            }
        }
        Results {
            total: hits.len(),
            hits,
            reference: true,
        }
    }

    fn search_words(&self, words: &[String]) -> Results {
        let mut hits = Vec::new();
        let mut total = 0;

        for (index, verse) in self.verses.iter().enumerate() {
            if verse.text.is_empty() || !words.iter().all(|w| verse.lowered.contains(w.as_str())) {
                continue;
            }
            total += 1;
            if hits.len() < MAX_HITS {
                let (book, chapter) = self.locations[index];
                hits.push(Hit {
                    book,
                    chapter,
                    verse: verse.number,
                    snippet: make_snippet(verse, words),
                });
            }
        }

        Results {
            hits,
            total,
            reference: false,
        }
    }
}

fn is_number_spec(word: &str) -> bool {
    !word.is_empty()
        && word.chars().any(|c| c.is_ascii_digit())
        && word.chars().all(|c| c.is_ascii_digit() || matches!(c, ':' | '-' | ','))
}

/// Reads `3`, `3:16`, `3:16-18` and `3:16,18` style chapter and verse lists.
fn parse_numbers(spec: &str) -> (Vec<u32>, Option<Vec<u32>>) {
    let parts: Vec<&str> = spec.split(',').collect();
    let lists_verses = parts.iter().any(|part| part.contains(':'));

    let mut chapters = Vec::new();
    let mut verses = Vec::new();
    for part in parts {
        match part.split_once(':') {
            Some((chapter, verse)) => {
                chapters.extend(parse_range(chapter));
                verses.extend(parse_range(verse));
            }
            // In "3:16,18" the bare numbers continue the verse list.
            None if lists_verses => verses.extend(parse_range(part)),
            None => chapters.extend(parse_range(part)),
        }
    }

    (chapters, (!verses.is_empty()).then_some(verses))
}

fn parse_range(text: &str) -> Vec<u32> {
    text.split('-')
        .filter_map(|part| part.trim().parse::<u32>().ok())
        .collect()
}

/// Builds the snippet shown in the result list, centred on the first match.
fn make_snippet(verse: &Verse, words: &[String]) -> String {
    const WINDOW: usize = 150;
    const LEAD: usize = 30;

    let position = words
        .iter()
        .filter_map(|word| verse.lowered.find(word.as_str()))
        .min()
        .unwrap_or(0);

    let start = verse.lowered[..position]
        .char_indices()
        .rev()
        .take(LEAD)
        .last()
        .map_or(0, |(index, _)| index);
    let mut end = (start + WINDOW).min(verse.text.len());
    while end > start && !verse.text.is_char_boundary(end) {
        end -= 1;
    }

    let mut snippet = String::new();
    if start > 0 {
        snippet.push('…');
    }
    snippet.push_str(&verse.text[start..end]);
    if end < verse.text.len() {
        snippet.push('…');
    }
    snippet
}

fn section_labels(names: &[&str]) -> Vec<&'static str> {
    let position = |needle: &str| {
        names
            .iter()
            .position(|name| *name == needle)
            .unwrap_or(usize::MAX)
    };
    let deuterocanonical = position("I Maccabees");
    let new_testament = position("Matthew");
    let apocrypha = position("Prayer of Manasses");

    names
        .iter()
        .enumerate()
        .map(|(index, _)| {
            if index >= apocrypha {
                SECTION_APOCRYPHA
            } else if index >= new_testament {
                SECTION_NT
            } else if index >= deuterocanonical {
                SECTION_DEUTEROCANONICAL
            } else {
                SECTION_OT
            }
        })
        .collect()
}

fn aliases_for(name: &str) -> Vec<String> {
    let mut aliases = Vec::new();
    let full = normalize(name);
    aliases.push(full.clone());
    if let Some(without_number) = strip_roman(&full) {
        aliases.push(without_number.to_string());
    }
    for extra in extra_aliases(name) {
        let extra = normalize(extra);
        if !aliases.contains(&extra) {
            aliases.push(extra);
        }
    }
    aliases
}

fn extra_aliases(name: &str) -> &'static [&'static str] {
    match name {
        "Psalms" => &["psalm", "ps"],
        "Song of Solomon" => &["song", "canticles"],
        "Revelation of John" => &["revelation", "rev", "apocalypse"],
        "Ecclesiastes" => &["eccl", "preacher"],
        "Lamentations" => &["lam"],
        "Isaiah" => &["isa"],
        "Zechariah" => &["zech"],
        "Obadiah" => &["obad"],
        _ => &[],
    }
}

/// Expands `1 john` into `i john` so that numbered book names match.
fn book_candidates(query: &str) -> Vec<String> {
    let query = query.trim();
    let mut candidates = vec![query.to_string()];
    for (digit, roman) in [("1", "i"), ("2", "ii"), ("3", "iii")] {
        let Some(rest) = query.strip_prefix(digit).map(str::trim_start) else {
            continue;
        };
        if rest.starts_with(|c: char| c.is_ascii_alphabetic()) {
            candidates.push(format!("{roman} {rest}"));
        }
    }
    candidates
}

fn strip_roman(normalized: &str) -> Option<&str> {
    ["iii", "ii", "i"]
        .into_iter()
        .find_map(|roman| {
            normalized
                .strip_prefix(roman)
                .filter(|rest| rest.starts_with(|c: char| c.is_ascii_alphabetic()))
        })
}

fn normalize(text: &str) -> String {
    text.chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEXT: &str = r#"{
      "books": [
        { "name": "Genesis", "chapters": [
          { "chapter": 1, "verses": [
            { "verse": 1, "text": "In the beginning God created heaven and earth." },
            { "verse": 2, "text": "And the earth was without form, and void." }
          ] },
          { "chapter": 2, "verses": [
            { "verse": 1, "text": "And the heavens and the earth were finished." },
            { "verse": 2, "text": "" }
          ] }
        ] },
        { "name": "Psalms", "chapters": [
          { "chapter": 23, "verses": [
            { "verse": 1, "text": "The LORD is my shepherd; I shall not want." },
            { "verse": 2, "text": "He maketh me to lie down in green pastures." }
          ] }
        ] },
        { "name": "I Maccabees", "chapters": [
          { "chapter": 1, "verses": [ { "verse": 1, "text": "Then arose Mattathias." } ] }
        ] },
        { "name": "Matthew", "chapters": [
          { "chapter": 1, "verses": [
            { "verse": 1, "text": "The book of the generation of Jesus Christ." }
          ] }
        ] },
        { "name": "Mark", "chapters": [] },
        { "name": "John", "chapters": [
          { "chapter": 3, "verses": [
            { "verse": 16, "text": "For God so loved the world, that he gave his only begotten Son." },
            { "verse": 17, "text": "And that is not the witness of John, when the Jews sent priests." }
          ] }
        ] },
        { "name": "I John", "chapters": [
          { "chapter": 2, "verses": [
            { "verse": 1, "text": "My little children, these things I write to you." }
          ] }
        ] }
      ]
    }"#;

    fn bible() -> Bible {
        Bible::from_reader(TEXT.as_bytes()).expect("test text parses")
    }

    fn passage(query: &str) -> (u32, Vec<u32>, Option<Vec<u32>>) {
        match bible().parse_query(query) {
            Some(Query::Passage {
                book,
                chapters,
                verses,
            }) => (book, chapters, verses),
            other => panic!("expected a passage for {query:?}, got {other:?}"),
        }
    }

    #[test]
    fn loads_books_with_sections() {
        let bible = bible();
        assert_eq!(bible.book_count(), 7);
        assert_eq!(bible.verse_count(), 11);
        assert_eq!(bible.books()[0].section, SECTION_OT);
        assert_eq!(bible.books()[2].section, SECTION_DEUTEROCANONICAL);
        assert_eq!(bible.books()[3].section, SECTION_NT);
        assert_eq!(bible.books()[0].verse_count(), 4);
        assert_eq!(bible.books()[0].chapters[0].verse_count(), 2);
    }

    #[test]
    fn finds_books_by_name_prefix_and_number() {
        let bible = bible();
        assert_eq!(bible.find_book("genesis"), Some(0));
        assert_eq!(bible.find_book("Gen"), Some(0));
        assert_eq!(bible.find_book("psalm"), Some(1));
        assert_eq!(bible.find_book("ps"), Some(1));
        assert_eq!(bible.find_book("1 macc"), Some(2));
        assert_eq!(bible.find_book("maccabees"), Some(2));
        // An exact name wins over the numbered books sharing it.
        assert_eq!(bible.find_book("john"), Some(5));
        assert_eq!(bible.find_book("1 john"), Some(6));
        assert_eq!(bible.find_book("nonsense"), None);
    }

    #[test]
    fn parses_passage_queries() {
        assert_eq!(passage("John 3"), (5, vec![3], None));
        assert_eq!(passage("john 3:16"), (5, vec![3], Some(vec![16])));
        assert_eq!(passage("gen 1:1-2"), (0, vec![1], Some(vec![1, 2])));
        assert_eq!(passage("ps 23:1,2"), (1, vec![23], Some(vec![1, 2])));
        assert_eq!(passage("gen 1,2"), (0, vec![1, 2], None));
        assert_eq!(passage("1 John 2:1"), (6, vec![2], Some(vec![1])));
    }

    #[test]
    fn falls_back_to_words() {
        assert!(matches!(
            bible().parse_query("green pastures"),
            Some(Query::Text(_))
        ));
        assert!(matches!(bible().parse_query("17"), Some(Query::Text(_))));
        assert!(bible().parse_query("   ").is_none());
    }

    #[test]
    fn searches_a_passage() {
        let results = bible().search(&bible().parse_query("john 3").unwrap());
        assert!(results.reference);
        assert_eq!(results.total, 2);
        assert_eq!(
            (results.hits[1].book, results.hits[1].chapter, results.hits[1].verse),
            (5, 3, 17)
        );
    }

    #[test]
    fn searches_words_in_any_order() {
        let results = bible().search(&bible().parse_query("son begotten").unwrap());
        assert!(!results.reference);
        assert_eq!(results.total, 1);
        assert_eq!(
            (results.hits[0].book, results.hits[0].chapter, results.hits[0].verse),
            (5, 3, 16)
        );
    }

    #[test]
    fn searches_ignore_empty_verses() {
        let results = bible().search(&bible().parse_query("heavens").unwrap());
        assert_eq!(results.total, 1);
    }

    #[test]
    fn walks_between_chapters_and_books() {
        let bible = bible();
        assert_eq!(bible.next_chapter(0, 1), Some((0, 2)));
        assert_eq!(bible.next_chapter(0, 2), Some((1, 23)));
        assert_eq!(bible.previous_chapter(1, 23), Some((0, 2)));
        assert_eq!(bible.previous_chapter(0, 1), None);
        assert_eq!(bible.next_chapter(6, 2), None);
    }

    #[test]
    fn formats_references() {
        let bible = bible();
        assert_eq!(bible.chapter_reference(5, 3), "John 3");
        assert_eq!(bible.reference(5, 3, 16), "John 3:16");
    }

    #[test]
    fn skips_chapters_the_data_does_not_have() {
        let text = bible();
        assert_eq!(passage("mark 1"), (4, vec![1], None));
        assert!(text.chapter_verses(4, 1).is_empty());
        assert!(text.search(&text.parse_query("mark 1").unwrap()).hits.is_empty());
        assert!(text.chapter_has_text(5, 3));
    }

    #[test]
    fn snippets_centre_on_the_match() {
        let words = vec!["shepherd".to_string()];
        let verse = Verse {
            number: 1,
            text: "The LORD is my shepherd; I shall not want.".to_string(),
            lowered: "the lord is my shepherd; i shall not want.".to_string(),
        };
        assert_eq!(make_snippet(&verse, &words), "The LORD is my shepherd; I shall not want.");

        let words = vec!["want".to_string()];
        assert!(make_snippet(&verse, &words).starts_with('…'));
    }
}
