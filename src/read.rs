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

    let mut text = String::new();
    let mut lines = Vec::new();

    for page in 0..pages {
        let spans = doc
            .extract_spans_with_reading_order(page, pdf_oxide::ReadingOrder::ColumnAware)
            .map_err(|e| format!("Failed to extract text: {}", e))?;

        let mut row: Vec<&pdf_oxide::layout::TextSpan> = Vec::new();
        for span in &spans {
            if span.text.trim().is_empty() {
                continue;
            }
            let broke = row
                .last()
                .is_some_and(|last: &&pdf_oxide::layout::TextSpan| {
                    starts_new_row(placed_at(last), placed_at(span))
                });
            if broke {
                push_line(&mut text, &mut lines, page + 1, &row);
                row.clear();
            }
            row.push(span);
        }
        push_line(&mut text, &mut lines, page + 1, &row);
    }

    Ok(Document { text, lines })
}

/// Where a span sits and how big it is set -- what deciding a line break needs.
#[derive(Debug, Clone, Copy)]
struct Placed {
    x: f32,
    y: f32,
    width: f32,
    font_size: f32,
}

/// Read a span's placement off the PDF.
fn placed_at(span: &pdf_oxide::layout::TextSpan) -> Placed {
    Placed {
        x: span.bbox.x,
        y: span.bbox.y,
        width: span.bbox.width,
        font_size: span.font_size,
    }
}

/// Whether `next` opens a new row rather than continuing the one `last` is on.
///
/// Three things end a row. The baseline moving is the obvious one. The other
/// two are what a shared baseline across two columns looks like, and reading
/// order alone does not separate them: a heading at the top of one column and
/// body text in the other are handed over one after the other, at the same
/// height, and joining them buries the heading inside a sentence.
///
/// A jump far wider than any word space says the text moved to another column
/// or block. Where the columns sit close enough that the jump is only a space
/// or two -- 16pt in one of the journals measured -- the type gives it away
/// instead: body text does not run into a heading half again its size
/// mid-line. A superscript is a size jump too, so that one only counts when
/// there is a gap; a superscript sits against the word it marks.
fn starts_new_row(last: Placed, next: Placed) -> bool {
    let larger = last.font_size.max(next.font_size);
    let smaller = last.font_size.min(next.font_size).max(0.1);

    // Measured against the larger of the two: a superscript is raised relative
    // to the line it sits on, not to its own small size, so a tolerance scaled
    // to the superscript reads the raise as a new line.
    if (last.y - next.y).abs() > (larger * 0.4).max(1.0) {
        return true;
    }

    let gap = next.x - (last.x + last.width);
    if gap.abs() > larger * COLUMN_JUMP {
        return true;
    }

    larger / smaller >= DISPLAY_JUMP && gap > smaller * 0.5
}

/// How far apart, in multiples of the type size, two spans have to be before
/// the space between them reads as a column break rather than a word space.
/// Measured over the corpus: table cells sit 3 to 5 across, column breaks 11
/// and up.
const COLUMN_JUMP: f32 = 6.0;

/// How much larger one span has to be set than its neighbour before they
/// cannot be on one line. The headings this separates run 1.5 and 1.67 times
/// the body they were joined to.
const DISPLAY_JUMP: f32 = 1.4;

/// Append one rebuilt row to the text, recording where it landed.
fn push_line(
    text: &mut String,
    lines: &mut Vec<Line>,
    page: usize,
    row: &[&pdf_oxide::layout::TextSpan],
) {
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

    let offset = text.len();
    text.push_str(trimmed);
    text.push('\n');
    lines.push(Line {
        text: trimmed.to_string(),
        font_size,
        bold,
        offset,
        page,
        y: first.bbox.y,
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
    /// The page it was set on, numbered from one.
    pub page: usize,
    /// The baseline it sits on, in PDF points up from the foot of the page.
    /// With `page`, this is what finds the line on the page again.
    pub y: f32,
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
    /// The page the heading was printed on, numbered from one.
    pub page: usize,
}

/// The section a heading line names, or `None` if the line is not a heading.
///
/// Numbering is dropped first. It arrives in several shapes -- `2. Methods`,
/// `3.1 Results`, `II.METHODS` -- and the numeral has to be split off at a
/// delimiter rather than by consuming roman-numeral characters, or the `I` of
/// `INTRODUCTION` gets eaten along with the `I.` in front of it.
fn canonical_section(text: &str) -> Option<&'static str> {
    // Hyphenation points and joiners are set inside the heading itself in some
    // PDFs -- Nature hands back `Methods` with a soft hyphen between every
    // other letter -- and none of them are meant to be read.
    let visible: String = text
        .chars()
        .filter(|c| {
            !matches!(
                c,
                '\u{ad}' | '\u{200b}' | '\u{200c}' | '\u{200d}' | '\u{2060}' | '\u{feff}'
            )
        })
        .collect();
    let mut rest = visible.trim();
    loop {
        // Digits can be taken without a delimiter. LaTeX sets the number apart
        // from the heading by position rather than by a space character, so the
        // extractor hands back `1Introduction`, and no section is named with a
        // digit anyway.
        let after_digits = rest.trim_start_matches(|c: char| c.is_ascii_digit());
        if after_digits.len() < rest.len() {
            rest = after_digits.trim_start_matches(['.', ' ']);
            continue;
        }
        // A roman numeral has to be delimited, or stripping it takes the `I`
        // of `INTRODUCTION` with it.
        let Some(split) = rest.find(['.', ' ']) else {
            break;
        };
        let (token, tail) = rest.split_at(split);
        if token.is_empty() || !is_roman_numeral(token) {
            break;
        }
        rest = tail[1..].trim_start();
    }
    let normalised = rest.to_lowercase();
    if let Some((_, section)) = HEADING_ALIASES
        .iter()
        .find(|(printed, _)| *printed == normalised)
    {
        return Some(section);
    }
    // Word spacing is positioning rather than space characters, and some PDFs
    // return none of it: `2. Materials and Methods` arrives as
    // `2.MaterialsandMethods`. Comparing without the spaces costs nothing here,
    // because the whole line has to be the heading either way.
    let unspaced: String = normalised.chars().filter(|c| !c.is_whitespace()).collect();
    HEADING_ALIASES
        .iter()
        .find(|(printed, _)| printed.replace(' ', "") == unspaced)
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
    ("discussions", "discussion"),
    ("conclusion", "conclusion"),
    ("conclusions", "conclusion"),
    ("references", "references"),
];

/// Whether a token is a roman section number: `II`, `IV`.
fn is_roman_numeral(token: &str) -> bool {
    token
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
            page: line.page,
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
                    page: 1,
                    y: 700.0 - offset as f32,
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
            (
                "Emerging evidence supports AI-ECG for detecting AMI.",
                8.7,
                false,
            ),
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
            (
                "Acute myocardial infarction remains a major cause.",
                9.5,
                false,
            ),
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

    // LaTeX sets the number and the heading apart with positioning, not with a
    // space character, so the extractor hands back `1Introduction`.
    #[test]
    fn detect_headings_strips_a_numbering_prefix_with_no_separator() {
        let doc = lines(&[("1Introduction", 12.0, true), ("3.1Results", 12.0, true)]);
        let found: Vec<&str> = detect_headings(&doc).iter().map(|h| h.section).collect();
        assert_eq!(found, vec!["introduction", "results"]);
    }

    // The roman numeral still needs its delimiter: without one, stripping
    // leading roman characters eats the `I` of `INTRODUCTION`.
    #[test]
    fn detect_headings_keeps_a_word_that_opens_with_roman_letters() {
        let doc = lines(&[("I.INTRODUCTION", 12.0, true)]);
        let found = detect_headings(&doc);
        assert_eq!(found.len(), 1, "got: {:?}", found);
        assert_eq!(found[0].section, "introduction");
    }

    // ── rows ─────────────────────────────────────

    fn placed(x: f32, y: f32, width: f32, font_size: f32) -> Placed {
        Placed {
            x,
            y,
            width,
            font_size,
        }
    }

    #[test]
    fn a_span_further_along_the_same_line_continues_the_row() {
        let last = placed(209.6, 329.4, 84.1, 9.0);
        let next = placed(296.0, 329.4, 20.0, 9.0);
        assert!(!starts_new_row(last, next));
    }

    #[test]
    fn a_span_on_a_new_baseline_opens_a_row() {
        let last = placed(42.5, 329.4, 84.1, 9.0);
        let next = placed(42.5, 316.0, 84.1, 9.0);
        assert!(starts_new_row(last, next));
    }

    // Communications Medicine sets `Plain language summary` at the top of the
    // right column and `Abstract` at the top of the left, on one baseline. The
    // reading-order pass hands them over in that order, and joining them hid
    // the abstract heading inside `Plain language summaryAbstract`.
    #[test]
    fn a_span_that_jumps_to_another_column_opens_a_row() {
        let last = placed(395.4, 690.0, 111.8, 10.0);
        let next = placed(39.7, 690.0, 35.0, 10.0);
        assert!(starts_new_row(last, next), "-467pt is not a word space");
    }

    // The columns of this journal are only 16pt apart, so the jump alone does
    // not give it away. The type does: 9pt body does not run into a 15pt
    // heading mid-line.
    #[test]
    fn body_text_does_not_run_into_a_display_heading() {
        let last = placed(209.6, 329.4, 84.1, 9.0);
        let next = placed(309.5, 325.4, 60.0, 15.0);
        assert!(starts_new_row(last, next), "9pt into 15pt is a new row");
    }

    // A superscript is a size jump too, but it sits right against the word it
    // marks rather than a space away.
    #[test]
    fn a_superscript_stays_on_its_line() {
        let last = placed(100.0, 329.4, 30.0, 9.0);
        let next = placed(130.2, 332.0, 4.0, 6.0);
        assert!(!starts_new_row(last, next));
    }

    // Table cells sit several spaces apart on a shared baseline, and the row
    // they form is a real line of the document.
    #[test]
    fn table_cells_stay_on_one_row() {
        let last = placed(100.0, 400.0, 40.0, 9.0);
        let next = placed(180.0, 400.0, 30.0, 9.0);
        assert!(
            !starts_new_row(last, next),
            "40pt is 4.4x the type, still a cell gap"
        );
    }

    // IEEE Transactions on Biomedical Engineering heads the section
    // `IV. DISCUSSIONS`, plural.
    #[test]
    fn detect_headings_accepts_a_plural_discussions_heading() {
        let doc = lines(&[("IV. DISCUSSIONS", 12.0, true)]);
        let found = detect_headings(&doc);
        assert_eq!(found.len(), 1, "got: {:?}", found);
        assert_eq!(found[0].section, "discussion");
    }

    // Word spacing is positioning, not spaces, and some PDFs give none of it
    // back: `2. Materials and Methods` arrives as `2.MaterialsandMethods`.
    #[test]
    fn detect_headings_accepts_a_heading_whose_spaces_were_lost() {
        let doc = lines(&[("2.MaterialsandMethods", 12.0, true)]);
        let found = detect_headings(&doc);
        assert_eq!(found.len(), 1, "got: {:?}", found);
        assert_eq!(found[0].section, "methods");
    }

    // Nature hyphenates inside the heading itself, so `Methods` comes back
    // carrying soft hyphens between its letters.
    #[test]
    fn detect_headings_ignores_invisible_formatting_inside_a_heading() {
        let doc = lines(&[("M\u{ad}et\u{ad}ho\u{ad}ds", 12.0, true)]);
        let found = detect_headings(&doc);
        assert_eq!(found.len(), 1, "got: {:?}", found);
        assert_eq!(found[0].section, "methods");
    }

    // A byte offset says where a heading is in the extracted text; the page
    // says where to look in the PDF. Checking the reading against the page is
    // the only way to know it is right, so the page has to come back with it.
    #[test]
    fn extract_document_reports_the_page_each_line_came_from() {
        let doc = extract_document(&fixture_path()).unwrap();
        assert!(!doc.lines.is_empty());
        assert_eq!(doc.lines[0].page, 1, "pages are numbered from one");
        assert!(
            doc.lines.windows(2).all(|w| w[0].page <= w[1].page),
            "pages should not go backwards"
        );
    }

    // Page plus baseline is what it takes to find a line on the page again --
    // to crop it out of a render and check the reading against the print.
    #[test]
    fn extract_document_reports_where_on_the_page_each_line_sits() {
        let doc = extract_document(&fixture_path()).unwrap();
        assert!(
            doc.lines.iter().all(|l| l.y > 0.0),
            "baselines should be set"
        );
        let first_page: Vec<&Line> = doc.lines.iter().filter(|l| l.page == 1).collect();
        assert!(
            first_page.windows(2).all(|w| w[0].y >= w[1].y),
            "a page is read from its top down, and PDF y counts from the bottom"
        );
    }
}
