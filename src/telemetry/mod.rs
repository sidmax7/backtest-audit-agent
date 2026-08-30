//! Trajectory recorder shared by `baseline` and `advanced` -- see
//! `agents.md` §4 for the schema and why this is mandatory, not incidental
//! logging. Steps are appended live as the binary actually executes them;
//! nothing here reconstructs a trace after the fact.

use anyhow::{Context, Result};
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::time::Instant;

#[derive(Serialize, Default, Clone, Copy)]
pub struct TokensUsed {
    pub input: u32,
    pub output: u32,
}

#[derive(Serialize)]
#[serde(tag = "type")]
pub enum TrajectoryStep {
    #[serde(rename = "reasoning")]
    Reasoning { step: usize, content: String },
    #[serde(rename = "tool_call")]
    ToolCall {
        step: usize,
        tool: String,
        input: serde_json::Value,
    },
    #[serde(rename = "tool_output")]
    ToolOutput {
        step: usize,
        output: serde_json::Value,
    },
    #[serde(rename = "verification")]
    Verification { step: usize, content: String },
    #[serde(rename = "final_verdict")]
    FinalVerdict {
        step: usize,
        verdict: String,
        sharpe_delta: f64,
        confidence: String,
    },
}

#[derive(Serialize)]
pub struct Trajectory {
    pub run_id: String,
    pub binary: String,
    pub strategy: String,
    /// Which LLM provider produced this run's verdict, e.g. `"anthropic"` or
    /// `"gemini"` -- `"none"` for `audit_csv`, which makes no LLM call at
    /// all. Without this, nothing in the trajectory file itself proves
    /// which model (or whether any model) was actually involved; a reader
    /// otherwise has to take the running `.env` on faith.
    pub provider: String,
    /// The exact model string sent on every request, e.g.
    /// `"gemini-3.5-flash-lite"`. `"n/a"` for `audit_csv`.
    pub model: String,
    pub started_at: String,
    pub steps: Vec<TrajectoryStep>,
    pub tokens_used: TokensUsed,
    pub wall_clock_ms: u128,
    /// Set on write, alongside `wall_clock_ms` -- an independent,
    /// human-checkable pair with `started_at`: the gap between them should
    /// roughly match `wall_clock_ms`, which is harder to fake by hand than
    /// either field alone.
    pub finished_at: String,
    #[serde(skip)]
    start_instant: Instant,
    #[serde(skip)]
    next_step: usize,
}

impl Trajectory {
    pub fn new(binary: &str, strategy: &str, provider: &str, model: &str) -> Self {
        Trajectory {
            run_id: uuid::Uuid::new_v4().to_string(),
            binary: binary.to_string(),
            strategy: strategy.to_string(),
            provider: provider.to_string(),
            model: model.to_string(),
            started_at: chrono::Utc::now().to_rfc3339(),
            steps: Vec::new(),
            tokens_used: TokensUsed::default(),
            wall_clock_ms: 0,
            finished_at: String::new(),
            start_instant: Instant::now(),
            next_step: 1,
        }
    }

    fn next(&mut self) -> usize {
        let s = self.next_step;
        self.next_step += 1;
        s
    }

    pub fn record_reasoning(&mut self, content: impl Into<String>) {
        let step = self.next();
        self.steps.push(TrajectoryStep::Reasoning {
            step,
            content: content.into(),
        });
    }

    pub fn record_tool_call(&mut self, tool: impl Into<String>, input: serde_json::Value) {
        let step = self.next();
        self.steps.push(TrajectoryStep::ToolCall {
            step,
            tool: tool.into(),
            input,
        });
    }

    pub fn record_tool_output(&mut self, output: serde_json::Value) {
        let step = self.next();
        self.steps.push(TrajectoryStep::ToolOutput { step, output });
    }

    pub fn record_verification(&mut self, content: impl Into<String>) {
        let step = self.next();
        self.steps.push(TrajectoryStep::Verification {
            step,
            content: content.into(),
        });
    }

    pub fn record_final_verdict(
        &mut self,
        verdict: impl Into<String>,
        sharpe_delta: f64,
        confidence: impl Into<String>,
    ) {
        let step = self.next();
        self.steps.push(TrajectoryStep::FinalVerdict {
            step,
            verdict: verdict.into(),
            sharpe_delta,
            confidence: confidence.into(),
        });
    }

    pub fn add_tokens(&mut self, input: u32, output: u32) {
        self.tokens_used.input += input;
        self.tokens_used.output += output;
    }

    /// Finalizes wall-clock time and writes the trajectory to
    /// `<dir>/<binary>_<strategy>_<unix-timestamp>.json`. Returns the path
    /// written, per the naming convention in `agents.md` §4.
    pub fn finish_and_write(mut self, dir: &Path) -> Result<PathBuf> {
        self.wall_clock_ms = self.start_instant.elapsed().as_millis();
        self.finished_at = chrono::Utc::now().to_rfc3339();
        std::fs::create_dir_all(dir)
            .with_context(|| format!("creating trajectory dir {}", dir.display()))?;
        let unix_ts = chrono::Utc::now().timestamp();
        let filename = format!("{}_{}_{}.json", self.binary, self.strategy, unix_ts);
        let path = dir.join(filename);
        let json = serde_json::to_string_pretty(&self)?;
        std::fs::write(&path, json).with_context(|| format!("writing {}", path.display()))?;
        Ok(path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_a_trajectory_file_with_recorded_steps() {
        let mut traj = Trajectory::new("advanced", "test_strategy", "anthropic", "claude-opus-5");
        traj.record_reasoning("suspect a forward-looking window");
        traj.record_tool_call("shift_test", serde_json::json!({"shift_bars": 1}));
        traj.record_tool_output(serde_json::json!({"sharpe_original": 9.4, "sharpe_shifted": 6.7}));
        traj.record_verification("delta below threshold -> leaky");
        traj.record_final_verdict("LEAKY", 2.7, "high");
        traj.add_tokens(120, 45);

        let dir = std::env::temp_dir().join(format!("traj-test-{}", uuid::Uuid::new_v4()));
        let path = traj.finish_and_write(&dir).unwrap();
        let contents = std::fs::read_to_string(&path).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&contents).unwrap();

        assert_eq!(parsed["binary"], "advanced");
        assert_eq!(parsed["strategy"], "test_strategy");
        assert_eq!(parsed["provider"], "anthropic");
        assert_eq!(parsed["model"], "claude-opus-5");
        assert!(!parsed["finished_at"].as_str().unwrap().is_empty());
        assert_eq!(parsed["steps"].as_array().unwrap().len(), 5);
        assert_eq!(parsed["steps"][0]["type"], "reasoning");
        assert_eq!(parsed["steps"][4]["type"], "final_verdict");
        assert_eq!(parsed["tokens_used"]["input"], 120);

        std::fs::remove_dir_all(&dir).unwrap();
    }
}
