//! Adapter for auditing a strategy this crate did not author.
//!
//! `verify::shift_test`'s wrapper takes a `&dyn Strategy`, which means the
//! agent binaries can only audit strategies registered in
//! `strategies::all_strategies()` -- fine for the seeded battery, useless
//! for the actual target user, who has their own backtest in their own
//! language. But the perturbation underneath
//! (`verify::shift_test_features`) never needed the trait: it operates on
//! a price series and a feature array. So a user who can export two
//! columns from *any* backtest -- pandas, R, a spreadsheet -- can have the
//! same empirical check run against their real numbers, with no Rust code
//! and no recompilation.
//!
//! Expected format: a header row, then numeric rows. Required columns:
//! `close`, plus at least one column whose name starts with `feature`
//! (several are allowed -- each is shift-tested independently, reusing
//! `verify::localize`'s per-component idea). `open`/`high`/`low`/`volume`
//! are optional and default to `close`/`close`/`close`/`0.0`; they exist
//! only so the `Bar` type is satisfiable, and none of them affect the
//! shift-test, which is close-to-close.
//!
//! This is a deliberately small parser: it splits on commas and parses
//! `f64`, which covers `DataFrame.to_csv()` output for numeric data. It
//! does not implement RFC-4180 quoting, so a quoted field containing a
//! comma will be misread -- that cannot occur in an all-numeric export,
//! and the parser errors loudly on any cell it cannot parse rather than
//! silently coercing it.

use anyhow::{bail, Context, Result};
use std::path::Path;

use crate::engine::Bar;

/// A price series plus one or more named feature columns, loaded from CSV.
#[derive(Debug, Clone)]
pub struct CsvSeries {
    pub bars: Vec<Bar>,
    /// `(column name, values)`, one entry per `feature*` column, in file order.
    pub features: Vec<(String, Vec<f64>)>,
}

fn parse_header(line: &str) -> Vec<String> {
    line.split(',')
        .map(|h| h.trim().trim_matches('"').to_lowercase())
        .collect()
}

pub fn load(path: impl AsRef<Path>) -> Result<CsvSeries> {
    let path = path.as_ref();
    let text =
        std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    parse(&text).with_context(|| format!("parsing {}", path.display()))
}

pub fn parse(text: &str) -> Result<CsvSeries> {
    let mut lines = text.lines().filter(|l| !l.trim().is_empty());
    let header = lines.next().context("empty file: no header row")?;
    let cols = parse_header(header);

    let close_idx = cols
        .iter()
        .position(|c| c == "close")
        .context("no `close` column found in header (required)")?;
    let feature_idxs: Vec<usize> = cols
        .iter()
        .enumerate()
        .filter(|(_, c)| c.starts_with("feature"))
        .map(|(i, _)| i)
        .collect();
    if feature_idxs.is_empty() {
        bail!("no column whose name starts with `feature` found in header (at least one required)");
    }
    let optional = |name: &str| cols.iter().position(|c| c == name);
    let (open_i, high_i, low_i, vol_i) = (
        optional("open"),
        optional("high"),
        optional("low"),
        optional("volume"),
    );

    let mut bars = Vec::new();
    let mut feature_cols: Vec<Vec<f64>> = vec![Vec::new(); feature_idxs.len()];

    for (row_no, line) in lines.enumerate() {
        let cells: Vec<&str> = line.split(',').map(|c| c.trim()).collect();
        // Row number reported to the user counts the header as line 1 and is
        // 1-based, so it matches what they'd see in an editor.
        let line_no = row_no + 2;
        let get = |idx: usize| -> Result<f64> {
            let cell = cells.get(idx).with_context(|| {
                format!(
                    "line {line_no}: expected {} columns, found {}",
                    cols.len(),
                    cells.len()
                )
            })?;
            cell.parse::<f64>().with_context(|| {
                format!(
                    "line {line_no}, column `{}`: cannot parse {cell:?} as a number",
                    cols[idx]
                )
            })
        };

        let close = get(close_idx)?;
        let open = match open_i {
            Some(i) => get(i)?,
            None => close,
        };
        let high = match high_i {
            Some(i) => get(i)?,
            None => close,
        };
        let low = match low_i {
            Some(i) => get(i)?,
            None => close,
        };
        let volume = match vol_i {
            Some(i) => get(i)?,
            None => 0.0,
        };
        bars.push(Bar {
            open,
            high,
            low,
            close,
            volume,
        });

        for (slot, &idx) in feature_idxs.iter().enumerate() {
            feature_cols[slot].push(get(idx)?);
        }
    }

    if bars.len() < 3 {
        bail!(
            "need at least 3 data rows to compute a meaningful Sharpe ratio, found {}",
            bars.len()
        );
    }

    let features = feature_idxs
        .iter()
        .zip(feature_cols)
        .map(|(&i, vals)| (cols[i].clone(), vals))
        .collect();

    Ok(CsvSeries { bars, features })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_minimal_close_and_feature() {
        let s = parse("close,feature\n100,0.5\n101,-0.2\n102,0.1\n").unwrap();
        assert_eq!(s.bars.len(), 3);
        assert_eq!(s.features.len(), 1);
        assert_eq!(s.features[0].0, "feature");
        assert_eq!(s.features[0].1, vec![0.5, -0.2, 0.1]);
        // open/high/low default to close; volume to 0.
        assert_eq!(s.bars[0].open, 100.0);
        assert_eq!(s.bars[0].volume, 0.0);
    }

    #[test]
    fn parses_multiple_named_feature_columns_in_order() {
        let s = parse("close,feature_a,feature_b\n1,0.1,0.9\n2,0.2,0.8\n3,0.3,0.7\n").unwrap();
        let names: Vec<&str> = s.features.iter().map(|(n, _)| n.as_str()).collect();
        assert_eq!(names, vec!["feature_a", "feature_b"]);
        assert_eq!(s.features[1].1, vec![0.9, 0.8, 0.7]);
    }

    #[test]
    fn honours_full_ohlcv_when_present_and_ignores_unknown_columns() {
        let s = parse("date,open,high,low,close,volume,feature\n1,9,11,8,10,555,0.4\n1,9,11,8,10,555,0.4\n1,9,11,8,10,555,0.4\n").unwrap();
        assert_eq!(s.bars[0].open, 9.0);
        assert_eq!(s.bars[0].high, 11.0);
        assert_eq!(s.bars[0].low, 8.0);
        assert_eq!(s.bars[0].volume, 555.0);
        assert_eq!(s.features.len(), 1);
    }

    #[test]
    fn rejects_missing_close_column() {
        let e = parse("price,feature\n1,2\n").unwrap_err().to_string();
        assert!(e.contains("close"), "unexpected error: {e}");
    }

    #[test]
    fn rejects_missing_feature_column() {
        let e = parse("close,signal\n1,2\n").unwrap_err().to_string();
        assert!(e.contains("feature"), "unexpected error: {e}");
    }

    #[test]
    fn reports_the_offending_line_and_column_on_a_bad_cell() {
        let e = format!(
            "{:#}",
            parse("close,feature\n100,0.5\n101,oops\n102,0.1\n").unwrap_err()
        );
        assert!(e.contains("line 3"), "should name the bad line: {e}");
        assert!(e.contains("feature"), "should name the bad column: {e}");
    }

    #[test]
    fn rejects_a_series_too_short_to_be_meaningful() {
        let e = parse("close,feature\n1,0.1\n").unwrap_err().to_string();
        assert!(e.contains("at least 3"), "unexpected error: {e}");
    }
}
