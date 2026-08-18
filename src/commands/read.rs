use super::{CommandResult, chars_summary, emit, failed};
use crate::cli::{GlobalOpts, OutputFormat, ReadArgs, Section};
use crate::read as pdf;

pub fn run(args: &ReadArgs, global: &GlobalOpts) -> CommandResult {
    if !args.path.exists() {
        return Err(failed(format!(
            "No such file: {}\n`read` works on a local PDF. To fetch one first: fastpaper download <id>",
            args.path.display()
        )));
    }

    let full_text = pdf::extract_text(&args.path).map_err(failed)?;

    let text = match heading_for(args.section) {
        None => full_text,
        Some(heading) => pdf::extract_section(&full_text, heading).ok_or_else(|| {
            failed(format!(
                "No '{}' section found in {}",
                heading,
                args.path.display()
            ))
        })?,
    };

    let text = match args.max_length {
        Some(max) => text.chars().take(max).collect::<String>(),
        None => text,
    };

    let char_count = text.chars().count();
    let rendered = match global.format {
        OutputFormat::Json => serde_json::to_string_pretty(&serde_json::json!({
            "path": args.path.to_string_lossy(),
            "section": section_name(args.section),
            "content": { "full_text": text },
        }))
        .unwrap(),
        _ => text,
    };
    let summary = chars_summary(char_count);
    emit(&rendered, args.output.as_deref(), &summary, global.quiet)
}

/// The heading `extract_section` should look for, or `None` for the whole text.
fn heading_for(section: Section) -> Option<&'static str> {
    match section {
        Section::Full => None,
        Section::Abstract => Some("abstract"),
        Section::Introduction => Some("introduction"),
        Section::Methods => Some("methods"),
        Section::Results => Some("results"),
        Section::Discussion => Some("discussion"),
        Section::Conclusion => Some("conclusion"),
        Section::References => Some("references"),
    }
}

fn section_name(section: Section) -> &'static str {
    heading_for(section).unwrap_or("full")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn full_means_no_slicing() {
        assert_eq!(heading_for(Section::Full), None);
    }

    // Every --section value must map to a heading the extractor knows about,
    // otherwise the flag accepts a value it silently ignores.
    #[test]
    fn every_section_value_maps_to_a_known_heading() {
        for section in [
            Section::Abstract,
            Section::Introduction,
            Section::Methods,
            Section::Results,
            Section::Discussion,
            Section::Conclusion,
            Section::References,
        ] {
            let heading = heading_for(section).expect("should map to a heading");
            let paper = format!("Title\nAbstract\nx\n{}\ncontent here\n", heading);
            assert!(
                crate::read::extract_section(&paper, heading).is_some(),
                "{:?} maps to '{}' which the extractor does not find",
                section,
                heading
            );
        }
    }

    #[test]
    fn section_name_reports_full_for_whole_document() {
        assert_eq!(section_name(Section::Full), "full");
        assert_eq!(section_name(Section::Methods), "methods");
    }
}
