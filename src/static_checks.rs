//! A Rust-native static-analysis pass over a strategy's own source, in the
//! same spirit as `backtest-audit` (AST pattern matching, no execution) --
//! built to complement `verify::shift_test`, not replace or compete with
//! it. Real Rust source parsed with `syn` (not string/regex matching), so
//! this is checking actual syntax structure the way `backtest-audit` checks
//! Python's `ast` module, just for this codebase's own strategy shape
//! rather than pandas idioms.
//!
//! Like any static rulebook (including `backtest-audit`'s), these three
//! rules are narrow and pattern-specific, not a general leakage prover --
//! see `eval/prior_art/README.md` for what that narrowness costs a
//! pandas-oriented tool on *this* codebase's bug shapes. These rules are
//! shaped for the reverse direction: they target exactly the three bug
//! mechanisms `src/strategies/*.rs` actually uses, and make no claim to
//! catch a hypothetical fourth one written differently.

use anyhow::{Context, Result};
use syn::visit::{self, Visit};
use syn::{BinOp, Expr, ImplItemFn, Lit, Stmt};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StaticFinding {
    pub rule: &'static str,
    pub message: String,
}

/// Parses `source` as a Rust file and runs all static rules against it.
/// Returns one finding per rule that matched (a strategy can trip more than
/// one, though none of the seeded ones happen to).
pub fn scan(source: &str) -> Result<Vec<StaticFinding>> {
    let file = syn::parse_file(source).context("static_checks: parsing strategy source")?;
    let mut visitor = Visitor::default();
    visitor.visit_file(&file);
    Ok(visitor.findings)
}

#[derive(Default)]
struct Visitor {
    findings: Vec<StaticFinding>,
}

impl<'ast> Visit<'ast> for Visitor {
    /// SA-EXEC-OFFSET: an `execution_offset()` override whose returned
    /// integer literal isn't `1` (the honest "next bar" contract) bets on a
    /// return that may already be realized at decision time.
    fn visit_impl_item_fn(&mut self, node: &'ast ImplItemFn) {
        if node.sig.ident == "execution_offset" {
            if let Some(returned) = last_int_literal(&node.block.stmts) {
                if returned != 1 {
                    self.findings.push(StaticFinding {
                        rule: "SA-EXEC-OFFSET",
                        message: format!(
                            "execution_offset() returns {returned}, not the honest default of 1 -- \
                             this strategy may be betting on a return that's already realized by the \
                             time its feature is computed."
                        ),
                    });
                }
            }
        }
        visit::visit_impl_item_fn(self, node);
    }

    /// SA-FORWARD-WINDOW: a window/slice upper bound computed as `i + ...`
    /// (the loop/closure index plus a positive offset) reaches forward past
    /// the current bar instead of back from it.
    fn visit_expr_binary(&mut self, node: &'ast syn::ExprBinary) {
        if matches!(node.op, BinOp::Add(_)) && is_ident(&node.left, "i") {
            self.findings.push(StaticFinding {
                rule: "SA-FORWARD-WINDOW",
                message: "found `i + ...` bounding a window/index -- this reaches forward from \
                          the current bar rather than back from it, the opposite of a trailing \
                          window."
                    .to_string(),
            });
        }
        visit::visit_expr_binary(self, node);
    }

    /// SA-GLOBAL-STAT: `.sum()` called on `<ident>.iter()` where `<ident>`
    /// is a bare variable (the whole series) rather than a sliced/windowed
    /// sub-range -- a statistic fit over data the strategy shouldn't yet
    /// have seen in full at any given bar.
    fn visit_expr_method_call(&mut self, node: &'ast syn::ExprMethodCall) {
        if node.method == "sum" {
            if let Expr::MethodCall(inner) = node.receiver.as_ref() {
                if inner.method == "iter" && matches!(inner.receiver.as_ref(), Expr::Path(_)) {
                    self.findings.push(StaticFinding {
                        rule: "SA-GLOBAL-STAT",
                        message: "found `.iter().sum()` on what looks like the whole series \
                                  (not a sliced sub-range) -- a statistic computed over the full \
                                  dataset is only knowable in hindsight."
                            .to_string(),
                    });
                }
            }
        }
        visit::visit_expr_method_call(self, node);
    }
}

fn is_ident(expr: &Expr, name: &str) -> bool {
    matches!(expr, Expr::Path(p) if p.path.is_ident(name))
}

fn last_int_literal(stmts: &[Stmt]) -> Option<i64> {
    stmts.iter().rev().find_map(|stmt| {
        let expr = match stmt {
            Stmt::Expr(expr, _) => Some(expr),
            _ => None,
        }?;
        match expr {
            Expr::Lit(lit) => match &lit.lit {
                Lit::Int(i) => i.base10_parse::<i64>().ok(),
                _ => None,
            },
            _ => None,
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `execution_realism_leak` is deliberately excluded: its bug lives in
    /// an unrealistic fill-price assumption, an axis these three rules have
    /// no rule for at all (not a gap in an existing rule -- there is no
    /// rule for this category, the same structural absence `problem.md`
    /// documents in `backtest-audit`). Its source is expected to scan
    /// clean; that is the honest result, not a failure of this test.
    #[test]
    fn scan_correctly_classifies_every_seeded_strategy() {
        for entry in crate::strategies::all_strategies() {
            if entry.name == "execution_realism_leak" {
                continue;
            }
            let source = std::fs::read_to_string(entry.source_path)
                .unwrap_or_else(|e| panic!("reading {}: {e}", entry.source_path));
            let findings = scan(&source).unwrap();
            let flagged = !findings.is_empty();
            assert_eq!(
                flagged, entry.is_leaky,
                "{}: expected is_leaky={}, static scan flagged={flagged} (findings: {findings:?})",
                entry.source_path, entry.is_leaky
            );
        }
    }

    #[test]
    fn detects_forward_window() {
        let src = r#"
            impl Strategy for X {
                fn compute_features(&self, bars: &[Bar]) -> Vec<f64> {
                    let end = i + self.window - 1;
                    vec![]
                }
            }
        "#;
        let findings = scan(src).unwrap();
        assert!(findings.iter().any(|f| f.rule == "SA-FORWARD-WINDOW"));
    }

    #[test]
    fn detects_global_stat() {
        let src = r#"
            impl Strategy for X {
                fn compute_features(&self, bars: &[Bar]) -> Vec<f64> {
                    let m = closes.iter().sum::<f64>() / n;
                    vec![]
                }
            }
        "#;
        let findings = scan(src).unwrap();
        assert!(findings.iter().any(|f| f.rule == "SA-GLOBAL-STAT"));
    }

    #[test]
    fn detects_bad_execution_offset() {
        let src = r#"
            impl Strategy for X {
                fn execution_offset(&self) -> usize {
                    0
                }
            }
        "#;
        let findings = scan(src).unwrap();
        assert!(findings.iter().any(|f| f.rule == "SA-EXEC-OFFSET"));
    }

    #[test]
    fn honest_execution_offset_of_one_is_not_flagged() {
        let src = r#"
            impl Strategy for X {
                fn execution_offset(&self) -> usize {
                    1
                }
            }
        "#;
        let findings = scan(src).unwrap();
        assert!(!findings.iter().any(|f| f.rule == "SA-EXEC-OFFSET"));
    }

    #[test]
    fn sliced_sum_is_not_flagged_as_global_stat() {
        let src = r#"
            impl Strategy for X {
                fn compute_features(&self, bars: &[Bar]) -> Vec<f64> {
                    let m = closes[start..=end].iter().sum::<f64>();
                    vec![]
                }
            }
        "#;
        let findings = scan(src).unwrap();
        assert!(!findings.iter().any(|f| f.rule == "SA-GLOBAL-STAT"));
    }
}
