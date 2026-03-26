use crate::sources::SearchResult;

/// Format and print a search result according to the chosen output format.
pub fn print_search_result(result: &SearchResult, format: &crate::cli::OutputFormat) {
    match format {
        crate::cli::OutputFormat::Json => {
            println!("{}", serde_json::to_string_pretty(result).unwrap());
        }
        crate::cli::OutputFormat::Jsonl => {
            for paper in &result.results {
                println!("{}", serde_json::to_string(paper).unwrap());
            }
        }
        _ => {
            // TODO: implement table, csv, bibtex output
            println!("{}", serde_json::to_string_pretty(result).unwrap());
        }
    }
}
