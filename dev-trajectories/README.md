# Dev Trajectories

This folder is distinct from [`trajectories/`](../trajectories/), which holds the *audited system's own* trajectories (the `baseline`/`advanced` binaries auditing seeded strategies — Deliverable 4, `instructions.md` §6). This folder instead discloses the *coding-agent trace of building the project itself*, per the separate requirement on the challenge page: coding-agent use is required, and its trajectories must be submitted for evaluation.

## What's in here

- **`claude-code-session-6db1bd94.redacted.jsonl`** — the raw session transcript of the Claude Code (Sonnet 5) conversation that produced the majority of this repository: the full crate build-out, all seeded strategies, the shift-test/static-scan/slippage-test verification tools, the LLM client, the evaluation harness, live evaluation runs, the prior-art comparison, uncertainty reporting, leak localization, the execution-realism axis, and this documentation. Claude Code was run as an extension inside Antigravity IDE.

## Redaction

The source file is `~/.claude/projects/-home-sidmax-Documents-micro1/6db1bd94-0eff-48a7-ae50-9cde4ac4c0f4.jsonl` on the machine this was built on. It is **not** included verbatim: during this same session, a real Gemini API key was pasted directly into chat for live LLM testing (see `CHANGELOG.md`), and that key appears in plaintext in the raw transcript. Before any part of this file was written, every line matching a credential-shaped pattern (the Gemini key itself, `GEMINI_API_KEY=...` assignments, and generic `*_API_KEY=...`/`Bearer ...`/`sk-...`-style tokens as a precaution against other formats) was replaced with a `[REDACTED-...]` placeholder. Each redacted batch was then re-scanned against the same patterns and confirmed to contain zero matches before being written here. Nothing else was altered — this is the real, unedited trace (reasoning, tool calls, tool outputs, in the model's own words) with only credentials removed.

## Two-part construction

The source transcript is this same session's own live log, which kept growing as the conversation continued past the point where a first export was taken — there is no way to capture "everything, including the message that requests the capture" from inside that same message. Rather than resubmit one single ever-growing snapshot, this file is two segments concatenated in chronological order:

1. **Lines 1–3030** — a full, contiguous export taken right at the `v1` git tag boundary (visible in the transcript itself: the last lines are the `git add`/tag/commit sequence for locking v1). Covers the crate build-out from scratch through the first 11 CHANGELOG iterations: the synthetic engine, all seeded strategies, the shift-test/static-scan/slippage-test verification tools, the LLM client, the evaluation harness, live evaluation runs, the prior-art comparison, uncertainty reporting, leak localization, and the execution-realism axis.
2. **Lines 3031 onward** — two further contiguous slices from later in the same live session, covering the remaining development work after `v1` was tagged: CHANGELOG Iteration 12 (the doc/reality drift verification pass), Iteration 13 (the CSV adapter and the false-positive it exposed), and Iteration 14 (trajectory self-certification — provider/model/timestamp fields). Deliberately excludes everything in between and after that isn't development on the audited system itself — the export-file creation step (self-referential for the same reason as above), conceptual Q&A with no resulting code change, and unrelated repo/video-production work — so this file stays scoped to genuine build-the-system trajectory, not the full session transcript.



