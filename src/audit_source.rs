//! Strips comments from a strategy's source before it's shown to an LLM.
//!
//! `src/strategies/*.rs` files carry doc comments explaining which
//! `problem.md` leakage category each one demonstrates and why -- genuinely
//! useful for a human reading the codebase, but a direct spoiler if handed
//! to the audit binaries verbatim (a comment literally starting "LEAKY.
//! Category 1..." makes "detection" trivial and invalidates the whole
//! baseline-vs-advanced comparison). Both binaries call this on the source
//! before it ever reaches a prompt.
//!
//! This is a naive line-based stripper (truncate each line at the first
//! `//`), not a real Rust lexer -- it would incorrectly truncate a line
//! containing `//` inside a string or char literal. That's fine for this
//! crate's own strategy files (none contain one), but this is not a
//! general-purpose Rust comment stripper.

pub fn strip_comments(source: &str) -> String {
    source
        .lines()
        .map(|line| match line.find("//") {
            Some(idx) => &line[..idx],
            None => line,
        })
        .filter(|line| !line.trim().is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_doc_comments_and_inline_comments() {
        let source =
            "//! LEAKY. Category 1.\nfn foo() {\n    let x = 1; // uses future data\n    x\n}\n";
        let stripped = strip_comments(source);
        assert!(!stripped.to_lowercase().contains("leaky"));
        assert!(!stripped.contains("future data"));
        assert!(stripped.contains("fn foo()"));
        assert!(stripped.contains("let x = 1;"));
    }

    #[test]
    fn every_seeded_strategy_file_is_spoiler_free_after_stripping() {
        for entry in crate::strategies::all_strategies() {
            let source = std::fs::read_to_string(entry.source_path)
                .unwrap_or_else(|e| panic!("reading {}: {e}", entry.source_path));
            let stripped = strip_comments(&source).to_lowercase();
            for spoiler in [
                "leaky",
                "clean control",
                "the bug",
                "look-ahead",
                "lookahead",
            ] {
                assert!(
                    !stripped.contains(spoiler),
                    "{} still contains spoiler text {spoiler:?} after stripping comments",
                    entry.source_path
                );
            }
        }
    }
}
