# Dev Trajectories

This folder is distinct from [`frontier-challenge/trajectories/`](../frontier-challenge/trajectories/), which holds the *audited system's own* trajectories (the `baseline`/`advanced` binaries auditing seeded strategies — Deliverable 4, `instructions.md` §6). This folder instead discloses the *coding-agent trace of building the project itself*, per the separate requirement on the challenge page: coding-agent use is required, and its trajectories must be submitted for evaluation.

## What's in here

- **`claude-code-session-6db1bd94.redacted.jsonl`** — the raw session transcript of the Claude Code (Sonnet 5) conversation that produced the majority of this repository: the full crate build-out, all seeded strategies, the shift-test/static-scan/slippage-test verification tools, the LLM client, the evaluation harness, live evaluation runs, the prior-art comparison, uncertainty reporting, leak localization, the execution-realism axis, and this documentation. Claude Code was run as an extension inside Antigravity IDE.

## Redaction

The source file is `~/.claude/projects/-home-sidmax-Documents-micro1/6db1bd94-0eff-48a7-ae50-9cde4ac4c0f4.jsonl` on the machine this was built on. It is **not** included verbatim: during this same session, a real Gemini API key was pasted directly into chat for live LLM testing (see `CHANGELOG.md`), and that key appears in plaintext in the raw transcript. Before this file was written, every line matching a credential-shaped pattern (the Gemini key itself, `GEMINI_API_KEY=...` assignments, and generic `*_API_KEY=...`/`Bearer ...`/`sk-...`-style tokens as a precaution against other formats) was replaced with a `[REDACTED-...]` placeholder. The redacted copy was then re-scanned against the same patterns and confirmed to contain zero matches before being written here. Nothing else was altered, trimmed, or curated — this is the real, unedited trace (reasoning, tool calls, tool outputs, in the model's own words) with only the credential removed.

**Known limitation:** the source transcript is this same session's own live log, which keeps growing as the conversation continues. This export is a point-in-time snapshot (taken after finishing V1), not a capture of the conversation through its actual end — there is no way to capture "everything, including the message that requests the capture" from inside that same message. This is disclosed rather than glossed over; it does not affect the transcript's validity as evidence of genuine agentic tool use, since a session's early and middle portions are exactly as verifiable as its last few turns.



