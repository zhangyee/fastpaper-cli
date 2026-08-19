use std::path::Path;

/// A PDF's text plus the typeset lines it was rebuilt from.
///
/// The two are built together so a line's `offset` always indexes into `text`;
/// that is what lets a heading found by its typography be turned into a slice.
pub struct Document {
    pub text: String,
    pub lines: Vec<Line>,
}

/// Read a PDF into text that remembers its own typography.
pub fn extract_document(path: &Path) -> Result<Document, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("Failed to read file: {}", e))?;
    extract_document_from_bytes(&bytes)
}

/// Read PDF bytes into text that remembers its own typography.
///
/// Spans come back in column-aware reading order rather than the order the
/// content stream happened to draw them in: a two-column journal otherwise
/// interleaves the columns, which strands headings in the middle of sentences.
pub fn extract_document_from_bytes(bytes: &[u8]) -> Result<Document, String> {
    let doc = pdf_oxide::PdfDocument::from_bytes(bytes.to_vec())
        .map_err(|e| format!("Failed to open PDF: {}", e))?;
    let pages = doc
        .page_count()
        .map_err(|e| format!("Failed to read page count: {}", e))?;

    let mut rows: Vec<Row> = Vec::new();
    let mut page_height = 0.0f32;

    for page in 0..pages {
        if let Ok(measured) = doc.extract_page_text(page) {
            page_height = page_height.max(measured.page_height);
        }
        let spans = doc
            .extract_spans_with_reading_order(page, pdf_oxide::ReadingOrder::ColumnAware)
            .map_err(|e| format!("Failed to extract text: {}", e))?;

        let mut row: Vec<&pdf_oxide::layout::TextSpan> = Vec::new();
        for span in &spans {
            if span.text.trim().is_empty() {
                continue;
            }
            // A new row starts when the baseline moves. The tolerance is a
            // fraction of the type size so superscripts and inline maths stay
            // on the line they belong to.
            let moved = row.last().is_some_and(|last| {
                (last.bbox.y - span.bbox.y).abs() > (span.font_size * 0.4).max(1.0)
            });
            if moved {
                push_row(&mut rows, page, &row);
                row.clear();
            }
            row.push(span);
        }
        push_row(&mut rows, page, &row);
    }

    // Furniture is dropped before the offsets are handed out, so a caller
    // never gets an offset pointing at a line it cannot see.
    let furniture = furniture_mask(&rows, page_height);
    let mut text = String::new();
    let mut lines = Vec::new();
    for (row, is_furniture) in rows.into_iter().zip(furniture) {
        if is_furniture {
            continue;
        }
        let offset = text.len();
        text.push_str(&row.text);
        text.push('\n');
        lines.push(Line {
            text: row.text,
            font_size: row.font_size,
            bold: row.bold,
            offset,
        });
    }

    Ok(Document { text, lines })
}

/// One row as it sits on its page, before page furniture is filtered out.
struct Row {
    page: usize,
    y: f32,
    text: String,
    font_size: f32,
    bold: bool,
}

/// Which rows are page furniture: running heads, folios, journal footers.
///
/// Two things have to hold together. The row has to repeat across pages --
/// compared with its digits collapsed, so `Page 3 of 18` and `Page 4 of 18`
/// count as the same row -- and its repeats have to sit at the same edge of
/// the page. Repetition alone is not enough: a figure's DOI or a bare numeral
/// repeats too, from wherever the figure happens to fall, and cutting those
/// would take real content out of the middle of the paper.
fn furniture_mask(rows: &[Row], page_height: f32) -> Vec<bool> {
    if page_height <= 0.0 {
        return vec![false; rows.len()];
    }
    let at_edge = |y: f32| {
        let fraction = y / page_height;
        fraction <= FOOT_BAND || fraction >= HEAD_BAND
    };

    let mut shapes: Vec<(String, Vec<usize>, usize, usize)> = Vec::new();
    for (index, row) in rows.iter().enumerate() {
        let shape = digits_collapsed(&row.text);
        match shapes.iter_mut().find(|(seen, ..)| *seen == shape) {
            Some((_, pages, seen, edge)) => {
                if !pages.contains(&row.page) {
                    pages.push(row.page);
                }
                *seen += 1;
                *edge += usize::from(at_edge(rows[index].y));
            }
            None => shapes.push((
                shape,
                vec![row.page],
                1,
                usize::from(at_edge(row.y)),
            )),
        }
    }

    rows.iter()
        .map(|row| {
            let shape = digits_collapsed(&row.text);
            shapes
                .iter()
                .find(|(seen, ..)| *seen == shape)
                .is_some_and(|(_, pages, seen, edge)| {
                    pages.len() >= FURNITURE_PAGES && edge * 10 >= seen * 8
                })
        })
        .collect()
}

/// Where the margins start, as a fraction of the page height. The two are not
/// symmetric because journals do not set them symmetrically: measured across
/// the corpus, footers sit below 0.07 and running heads from 0.90 up --
/// Cardiovascular Diagnosis and Therapy puts its folio at 0.91, which a band
/// mirrored from the foot would walk straight past.
const FOOT_BAND: f32 = 0.07;
const HEAD_BAND: f32 = 0.90;

/// How many pages a row has to repeat on before it reads as furniture.
const FURNITURE_PAGES: usize = 3;

/// A row with every run of digits replaced by one `#`, so a folio counting up
/// across the paper compares equal to itself.
fn digits_collapsed(text: &str) -> String {
    let mut out = String::new();
    let mut in_digits = false;
    for c in text.chars() {
        if c.is_ascii_digit() {
            if !in_digits {
                out.push('#');
                in_digits = true;
            }
        } else {
            out.push(c);
            in_digits = false;
        }
    }
    out.trim().to_string()
}

/// Collect one rebuilt row, keeping where on the page it sat.
fn push_row(rows: &mut Vec<Row>, page: usize, row: &[&pdf_oxide::layout::TextSpan]) {
    let Some(first) = row.first() else {
        return;
    };
    let joined: String = row.iter().map(|s| s.text.as_str()).collect();
    let trimmed = joined.trim();
    if trimmed.is_empty() {
        return;
    }
    // A heading's own span carries the display type; body spans on the same row
    // (a running head, a superscript) must not drag the size back down.
    let font_size = row.iter().fold(0.0f32, |acc, s| acc.max(s.font_size));
    let bold = row
        .iter()
        .any(|s| matches!(s.font_weight, pdf_oxide::layout::FontWeight::Bold));

    rows.push(Row {
        page,
        y: first.bbox.y,
        text: trimmed.to_string(),
        font_size,
        bold,
    });
}

// ── layout-driven heading detection ──────────────

/// One typeset line of a PDF, with the typography needed to tell a heading
/// from a sentence that merely contains the same word.
#[derive(Debug, Clone)]
pub struct Line {
    pub text: String,
    pub font_size: f32,
    pub bold: bool,
    /// Byte offset of this line's first character in the rebuilt full text.
    pub offset: usize,
}

/// A section heading located in a document.
#[derive(Debug, Clone)]
pub struct Heading {
    /// Canonical section name, e.g. `"methods"` for a `Materials and Methods`.
    pub section: &'static str,
    /// The heading line as it appears in the PDF.
    pub text: String,
    /// Byte offset of the heading line itself.
    pub offset: usize,
    /// Byte offset where the body starts, just past the heading line.
    pub body_start: usize,
    pub font_size: f32,
}

/// The section a heading line names, or `None` if the line is not a heading.
///
/// Numbering is dropped first. It arrives in several shapes -- `2. Methods`,
/// `3.1 Results`, `II.METHODS` -- and the numeral has to be split off at a
/// delimiter rather than by consuming roman-numeral characters, or the `I` of
/// `INTRODUCTION` gets eaten along with the `I.` in front of it.
fn canonical_section(text: &str) -> Option<&'static str> {
    let mut rest = text.trim();
    while let Some(split) = rest.find(['.', ' ']) {
        let (token, tail) = rest.split_at(split);
        if token.is_empty() || !is_numbering(token) {
            break;
        }
        rest = tail[1..].trim_start();
    }
    let normalised = rest.to_lowercase();
    HEADING_ALIASES
        .iter()
        .find(|(printed, _)| *printed == normalised)
        .map(|(_, section)| *section)
}

/// Headings as journals actually print them, mapped to the section they name.
///
/// One section goes by several names: EHJ Digital Health heads its methods
/// section `Method`, Elsevier prints `Materials and methods`.
///
/// Only sections `--section` can ask for appear here. A heading that names no
/// such section falls inside the one above it, which is how a subsection like
/// `Ethical approval` ends up part of the methods rather than cutting them
/// short.
///
/// `Background` maps to the introduction because journals use it for both:
/// some head the introduction itself with it, and Cardiovascular Diagnosis and
/// Therapy heads the introduction's first subsection with it -- set in the
/// same size as the chapter heading above, distinguished only by typeface and
/// colour, which is not something the extracted text records. Sending both to
/// `introduction` gives the same answer either way.
const HEADING_ALIASES: &[(&str, &str)] = &[
    ("abstract", "abstract"),
    ("introduction", "introduction"),
    ("background", "introduction"),
    ("method", "methods"),
    ("methods", "methods"),
    ("methodology", "methods"),
    ("materials and methods", "methods"),
    ("results", "results"),
    ("discussion", "discussion"),
    ("conclusion", "conclusion"),
    ("conclusions", "conclusion"),
    ("references", "references"),
];

/// Whether a token is a section number: `3`, `2`, `IV`.
fn is_numbering(token: &str) -> bool {
    token.chars().all(|c| c.is_ascii_digit())
        || token
            .chars()
            .all(|c| matches!(c.to_ascii_uppercase(), 'I' | 'V' | 'X' | 'L' | 'C'))
}

/// The type size the body of the document is set in.
///
/// Taken as the size carrying the most characters rather than the average:
/// running heads, captions and the title would all pull an average away from
/// the size that actually matters.
fn body_font_size(lines: &[Line]) -> f32 {
    let mut tally: Vec<(f32, usize)> = Vec::new();
    for line in lines {
        let size = (line.font_size * 10.0).round() / 10.0;
        match tally.iter_mut().find(|(seen, _)| *seen == size) {
            Some((_, chars)) => *chars += line.text.len(),
            None => tally.push((size, line.text.len())),
        }
    }
    tally
        .iter()
        .max_by_key(|(_, chars)| *chars)
        .map(|(size, _)| *size)
        .unwrap_or_default()
}

/// Locate the real section headings among a document's typeset lines.
///
/// A heading owns its line. That single rule is what separates `Methods` the
/// heading from `conventional interpretation methods has low reliability`, the
/// sentence the old substring search used to match first.
/// A structured abstract prints its labels on their own lines too, so owning a
/// line only makes a candidate. The body heading is the one set in display
/// type, so when a name repeats the largest type wins.
pub fn detect_headings(lines: &[Line]) -> Vec<Heading> {
    let body = body_font_size(lines);
    let mut best: Vec<Heading> = Vec::new();
    for line in lines {
        let text = line.text.trim();
        let Some(section) = canonical_section(text) else {
            continue;
        };
        // A footnote or figure label can read exactly `Methods` and own its
        // line. No journal sets a heading smaller than the body it heads, so
        // small type rules the line out. The margin absorbs papers that head
        // their sections in body-sized capitals.
        if line.font_size < body * 0.95 {
            continue;
        }
        let candidate = Heading {
            section,
            text: text.to_string(),
            offset: line.offset,
            body_start: line.offset + line.text.len(),
            font_size: line.font_size,
        };
        match best.iter_mut().find(|h| h.section == candidate.section) {
            Some(kept) if candidate.font_size > kept.font_size => *kept = candidate,
            Some(_) => {}
            None => best.push(candidate),
        }
    }
    best.sort_by_key(|h| h.offset);
    best
}


/// A located section: the heading that names it and the body beneath it.
#[derive(Debug, Clone)]
pub struct Section {
    pub heading: Heading,
    pub body: String,
}

/// Cut one section out of a document, or `None` if it has no such heading.
///
/// The body ends at the next heading of any kind, so it does not matter that
/// journals order their sections differently -- Nature runs results before
/// methods -- only that the headings were found.
pub fn find_section(doc: &Document, section: &str) -> Option<Section> {
    let headings = detect_headings(&doc.lines);
    let position = headings.iter().position(|h| h.section == section)?;
    let heading = headings[position].clone();
    let end = headings
        .get(position + 1)
        .map_or(doc.text.len(), |next| next.offset);
    let body = doc.text[heading.body_start..end].trim().to_string();
    Some(Section { heading, body })
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn fixture_path() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/test.pdf")
    }

    /// Lay out rows as consecutive lines, offsets as if joined by newlines.
    fn lines(rows: &[(&str, f32, bool)]) -> Vec<Line> {
        let mut offset = 0;
        rows.iter()
            .map(|(text, size, bold)| {
                let line = Line {
                    text: (*text).to_string(),
                    font_size: *size,
                    bold: *bold,
                    offset,
                };
                offset += text.len() + 1;
                line
            })
            .collect()
    }

    /// Build a document the way `extract_document` does, so line offsets index
    /// into the text.
    fn document(rows: &[(&str, f32, bool)]) -> Document {
        let lines = lines(rows);
        let mut text = String::new();
        for line in &lines {
            text.push_str(&line.text);
            text.push('\n');
        }
        Document { text, lines }
    }

    /// A paper laid out the way most journals set one: display-type headings
    /// over body-type prose.
    fn paper() -> Document {
        document(&[
            ("Title Here", 16.0, true),
            ("Abstract", 12.0, true),
            ("We present a thing.", 9.0, false),
            ("Introduction", 12.0, true),
            ("Prior work was slow.", 9.0, false),
            ("Methods", 12.0, true),
            ("We used a hammer.", 9.0, false),
            ("Results", 12.0, true),
            ("It worked.", 9.0, false),
            ("Conclusion", 12.0, true),
            ("Hammers are good.", 9.0, false),
            ("References", 12.0, true),
            ("[1] Someone, 2020.", 9.0, false),
        ])
    }

    // ── reading a PDF ────────────────────────────

    #[test]
    fn extract_document_returns_content() {
        let doc = extract_document(&fixture_path()).unwrap();
        assert!(!doc.text.is_empty(), "extracted text should not be empty");
    }

    #[test]
    fn extract_document_contains_expected_words() {
        let doc = extract_document(&fixture_path()).unwrap();
        assert!(
            doc.text.contains("Transformer")
                || doc.text.contains("attention")
                || doc.text.contains("Attention"),
            "text should contain expected words, got: {}",
            &doc.text[..doc.text.len().min(200)]
        );
    }

    #[test]
    fn extract_document_from_bytes_works() {
        let bytes = std::fs::read(fixture_path()).unwrap();
        let doc = extract_document_from_bytes(&bytes).unwrap();
        assert!(!doc.text.is_empty());
    }

    #[test]
    fn extract_document_nonexistent_file_returns_err() {
        let result = extract_document(Path::new("/nonexistent/fake.pdf"));
        assert!(result.is_err());
    }

    // The whole scheme rests on this: a heading located by its type must be
    // turnable into a slice of the text the caller reads.
    #[test]
    fn extract_document_line_offsets_index_into_its_text() {
        let doc = extract_document(&fixture_path()).unwrap();
        assert!(!doc.lines.is_empty(), "no lines rebuilt");
        for line in &doc.lines {
            let end = line.offset + line.text.len();
            assert_eq!(
                &doc.text[line.offset..end],
                line.text,
                "line at {} does not index into the text",
                line.offset
            );
        }
    }

    // ── layout-driven heading detection ──────────

    // The old extractor matched the bare substring "methods", so a Scientific
    // Reports abstract reading "conventional interpretation methods has low
    // reliability" beat the real heading eight pages later.
    #[test]
    fn detect_headings_ignores_a_heading_word_inside_a_sentence() {
        let doc = lines(&[
            (
                "conventional interpretation methods has low reliability",
                9.0,
                true,
            ),
            ("Methods", 11.0, true),
        ]);
        let found = detect_headings(&doc);
        assert_eq!(found.len(), 1, "got: {:?}", found);
        assert_eq!(found[0].section, "methods");
        assert_eq!(found[0].text, "Methods");
    }

    // European Heart Journal prints a structured abstract whose labels are
    // their own lines too, so owning a line is not enough -- the body heading
    // is the one set in display type.
    #[test]
    fn detect_headings_prefers_the_body_heading_over_the_abstract_label() {
        let doc = lines(&[
            ("Methods", 9.5, true),
            ("Introduction", 15.0, true),
            ("Methods", 15.0, true),
        ]);
        let found = detect_headings(&doc);
        let methods: Vec<_> = found.iter().filter(|h| h.section == "methods").collect();
        assert_eq!(methods.len(), 1, "got: {:?}", found);
        assert_eq!(methods[0].font_size, 15.0);
    }

    // IEEE-style papers number their headings, and the extractor gets no space
    // after the numeral: `II.METHODS`.
    #[test]
    fn detect_headings_strips_a_numbering_prefix() {
        let doc = lines(&[
            ("I.INTRODUCTION", 9.96, false),
            ("2. Methods", 9.96, false),
            ("3.1 Results", 9.96, false),
        ]);
        let found: Vec<&str> = detect_headings(&doc).iter().map(|h| h.section).collect();
        assert_eq!(found, vec!["introduction", "methods", "results"]);
    }

    // EHJ Digital Health heads its methods section `Method`, singular.
    #[test]
    fn detect_headings_accepts_a_singular_method_heading() {
        let doc = lines(&[("Method", 15.0, true)]);
        let found = detect_headings(&doc);
        assert_eq!(found.len(), 1, "got: {:?}", found);
        assert_eq!(found[0].section, "methods");
        assert_eq!(found[0].text, "Method", "the printed heading is kept as-is");
    }

    // A footnote or figure label can read exactly `Methods` and own its line.
    // Type size is what gives it away: a heading is never set smaller than the
    // body it heads.
    #[test]
    fn detect_headings_rejects_a_candidate_set_smaller_than_the_body() {
        let mut rows: Vec<(&str, f32, bool)> = (0..20)
            .map(|_| ("ordinary body text carrying most of the paper", 10.9, false))
            .collect();
        rows.push(("Methods", 5.1, false));
        let found = detect_headings(&lines(&rows));
        assert!(found.is_empty(), "got: {:?}", found);
    }

    #[test]
    fn detect_headings_is_case_insensitive() {
        let upper = detect_headings(&lines(&[("METHODS", 12.0, true)]));
        let lower = detect_headings(&lines(&[("methods", 12.0, true)]));
        assert_eq!(upper.len(), 1);
        assert_eq!(upper[0].section, lower[0].section);
    }

    // ── cutting a section out ────────────────────

    #[test]
    fn find_section_body_runs_to_the_next_heading() {
        let doc = document(&[
            ("Abstract", 12.0, true),
            ("We present a thing.", 9.0, false),
            ("Introduction", 12.0, true),
            ("Prior work was slow.", 9.0, false),
        ]);
        let found = find_section(&doc, "abstract").unwrap();
        assert_eq!(found.body.trim(), "We present a thing.");
        assert_eq!(found.heading.text, "Abstract");
    }

    #[test]
    fn find_section_abstract_does_not_run_into_the_introduction() {
        let found = find_section(&paper(), "abstract").unwrap();
        assert!(
            !found.body.to_lowercase().contains("introduction"),
            "got: {:?}",
            found.body
        );
    }

    #[test]
    fn find_section_conclusion_stops_before_the_references() {
        let found = find_section(&paper(), "conclusion").unwrap();
        assert!(found.body.contains("Hammers are good"), "got: {:?}", found);
        assert!(!found.body.contains("Someone, 2020"), "got: {:?}", found);
    }

    #[test]
    fn find_section_methods_returns_only_the_methods() {
        let found = find_section(&paper(), "methods").unwrap();
        assert!(found.body.contains("hammer"), "got: {:?}", found);
        assert!(!found.body.contains("It worked"), "got: {:?}", found);
    }

    #[test]
    fn find_section_returns_none_for_a_section_the_paper_lacks() {
        let doc = document(&[
            ("Abstract", 12.0, true),
            ("We present a thing.", 9.0, false),
        ]);
        assert!(find_section(&doc, "discussion").is_none());
    }

    // The last section runs to the end of the paper, not to nothing.
    #[test]
    fn find_section_references_runs_to_the_end() {
        let found = find_section(&paper(), "references").unwrap();
        assert!(found.body.contains("Someone, 2020"), "got: {:?}", found);
    }

    // A structured abstract's own labels sit right under the `Abstract`
    // heading. The old extractor treated the first of them as the next section
    // and returned an empty slice, which it then reported as "no abstract
    // found" -- for a paper whose abstract was right there.
    #[test]
    fn find_section_survives_a_label_directly_under_the_heading() {
        let doc = document(&[
            ("Abstract", 9.5, true),
            ("Background and Aims", 9.5, true),
            ("Emerging evidence supports AI-ECG for detecting AMI.", 8.7, false),
            ("Introduction", 15.0, true),
            ("Acute myocardial infarction is common.", 8.7, false),
        ]);
        let found = find_section(&doc, "abstract").unwrap();
        assert!(
            found.body.contains("Emerging evidence"),
            "got: {:?}",
            found.body
        );
    }

    // Cardiovascular Diagnosis and Therapy heads the introduction, then heads
    // its first subsection `Background` in the same type. Treating that as the
    // start of a new section left the introduction empty.
    #[test]
    fn find_section_introduction_absorbs_a_background_subsection() {
        let doc = document(&[
            ("Introduction", 9.5, true),
            ("Background", 9.5, true),
            ("Acute myocardial infarction remains a major cause.", 9.5, false),
            ("Methods", 9.5, true),
            ("We did things.", 9.5, false),
        ]);
        let found = find_section(&doc, "introduction").unwrap();
        assert!(
            found.body.contains("Acute myocardial infarction"),
            "got: {:?}",
            found.body
        );
    }

    // Most journals head the section `Conclusions`, plural.
    #[test]
    fn detect_headings_accepts_a_plural_conclusions_heading() {
        let doc = lines(&[("Conclusions", 15.0, true)]);
        let found = detect_headings(&doc);
        assert_eq!(found.len(), 1, "got: {:?}", found);
        assert_eq!(found[0].section, "conclusion");
    }

    // ── page furniture ───────────────────────────

    fn row(page: usize, y: f32, text: &str) -> Row {
        Row {
            page,
            y,
            text: text.to_string(),
            font_size: 9.0,
            bold: false,
        }
    }

    // A running head and a folio repeat at the same edge of page after page.
    // Left in, they splice themselves into the middle of a quoted sentence.
    #[test]
    fn furniture_mask_catches_a_running_head_and_a_folio() {
        let mut rows = Vec::new();
        for page in 0..4 {
            rows.push(row(page, 760.0, "www.nature.com/scientificreports/"));
            rows.push(row(page, 400.0, "a sentence from the paper itself"));
            rows.push(row(page, 18.0, &format!("Page {} of 4", page + 1)));
        }
        let mask = furniture_mask(&rows, 782.0);
        let kept: Vec<&str> = rows
            .iter()
            .zip(&mask)
            .filter(|(_, furniture)| !**furniture)
            .map(|(r, _)| r.text.as_str())
            .collect();
        assert_eq!(kept, vec!["a sentence from the paper itself"; 4], "kept: {:?}", kept);
    }

    // Repetition alone is not enough. A bare figure number or a table's DOI
    // repeats too, but from wherever the figure happens to sit, and cutting
    // those would take real content with them.
    #[test]
    fn furniture_mask_spares_text_that_repeats_away_from_the_page_edge() {
        let mut rows = Vec::new();
        for page in 0..4 {
            rows.push(row(page, 300.0 + page as f32 * 40.0, "https://doi.org/10.1371/journal.t001"));
        }
        let mask = furniture_mask(&rows, 792.0);
        assert!(mask.iter().all(|f| !f), "nothing here sits at a page edge");
    }

    // Journals do not agree on how deep the top margin is. Cardiovascular
    // Diagnosis and Therapy sets its folio at 0.91 of the page height, which a
    // band measured from the foot alone walks straight past.
    #[test]
    fn furniture_mask_reaches_a_folio_set_low_in_the_head_margin() {
        let rows: Vec<Row> = (0..4)
            .map(|page| row(page, 713.0, &format!("Page {} of 18", page + 2)))
            .collect();
        let mask = furniture_mask(&rows, 780.0);
        assert!(mask.iter().all(|f| *f), "0.91 of the page height is the head margin");
    }
}
