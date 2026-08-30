# Agent Operating Guidelines & Instructions (`instructions.md`)

> **Project:** micro1 Frontier Engineering Challenge 2026 / Agentic Workflows Hackathon  
> **Target Audience:** Any AI Model / Coding Agent working in this repository (e.g., Gemini 3.7, Claude 3.7 Sonnet, GPT-4.5 / o3, DeepSeek-R1, etc.)  
> **Primary Objective:** Build, evaluate, and document an award-winning agentic workflow that solves a real-world problem with measurable superiority over a naive baseline.

---

## 1. Core Operating Principles for All Agents

Whenever you are working in this repository, you must adhere to the following non-negotiable standards:

1. **Engineering Over Vibes:** Do not write superficial or hallucinated code. Every module must be functional, typed, testable, and deterministic.
2. **Always Preserve the Baseline vs. Solution Delta:** Every feature must be benchmarkable against our defined baseline. Never overwrite or eliminate baseline test cases.
3. **Trace & Trajectory Awareness:** Every agent action, tool invocation, input, and output must be structured and logged for submission traces.
4. **Clean Environment Reproducibility:** Ensure every script, test, or evaluation can run from a fresh environment via a single CLI command with clear dependency pinning.
5. **No Leaked Secrets or Consequential Unchecked Actions:** Use `.env.example` templates; never hardcode API keys or credentials. Sandbox all destructive operations and keep human approval gates for critical actions.

---

## 2. Model Switching & Capability Tiers

The user will switch between different AI models depending on the difficulty and nature of the task. Follow these protocols based on your assigned tier:

### 🟢 Tier 1: Fast Execution Models *(e.g., Flash / Mini / Haiku)*
* **Best Suited For:**
  * Writing boilerplate, standard utility functions, and unit tests.
  * Running evaluations and formatting tables / markdown reports.
  * Refactoring syntax, fixing linter errors, and updating documentation.
* **Protocol:**
  * Execute directly and concisely.
  * Keep modifications scoped to the exact files requested.
  * Do not unilaterally alter system architecture or core agent prompts.

### 🟣 Tier 2: Deep Reasoning & Architecture Models *(e.g., Gemini Pro/Thinking, Claude Sonnet/Opus, o3-mini/R1)*
* **Best Suited For:**
  * Designing multi-agent orchestration, state management, and memory schemas.
  * Implementing tool-use loops, reflection/verification steps, and error recovery.
  * Designing evaluation rubrics, edge-case test suites, and failure mode analysis.
  * Writing the **Improvement Changelog**, **Failure Modes Analysis**, and **Hot Takes**.
* **Protocol:**
  * Always review existing files and test results before making architectural decisions.
  * Explain *why* a particular design pattern was chosen over simpler alternatives.
  * When introducing a new experiment, register it in the `CHANGELOG.md` with hypotheses, metrics, and outcomes.

---

## 3. Context Onboarding Protocol (For Any Newly Activated Model)

When you are activated in this conversation or repository, follow this 3-step checklist before making edits:

```
┌─────────────────────────────────┐
│ 1. Read Project Context         │ ──► Check README.md, instructions.md,
│                                 │     and Frontier_Engineering_Challenge_2026.md
└────────────────┬────────────────┘
                 │
┌────────────────▼────────────────┐
│ 2. Check Changelog & Metrics    │ ──► Inspect current progress, benchmark results,
│                                 │     and latest active iteration
└────────────────┬────────────────┘
                 │
┌────────────────▼────────────────┐
│ 3. Execute with Telemetry       │ ──► Write code, run tests/evaluations,
│                                 │     log trajectories, and update logs
└─────────────────────────────────┘
```

1. **Inspect Existing Files:**
   * Review [instructions.md](instructions.md)
   * Review [Frontier_Engineering_Challenge_2026.md](Frontier_Engineering_Challenge_2026.md)
   * Review [micro1_Hackathon_Uno_Agentic_Workflows.md](micro1_Hackathon_Uno_Agentic_Workflows.md)
2. **Review Current Experiments:**
   * Check what baseline is established and what iteration is currently being tested.
3. **Verify Dependencies & Configs:**
   * Respect existing virtual environments, package managers, and configuration files.

---

## 4. Repository Structure & Standards

All code and artifacts must follow this directory layout:

```text
├── README.md                 # Project introduction, persona, bottleneck, architecture
├── REPRODUCTION.md           # Zero-to-hero clean setup and execution guide
├── CHANGELOG.md              # Iteration progression with metrics and learnings
├── instructions.md           # This agent guidance file
├── config/                   # Configuration files and prompt templates
│   ├── prompts/              # System prompts for agents and baselines
│   └── settings.py / .ts     # App configuration & constants
├── src/                      # Core agent solution code
│   ├── baseline/             # Naive single-prompt or basic script implementation
│   ├── agent/                # Multi-step / tool-augmented / orchestrated solution
│   │   ├── tools/            # Custom tools & integrations
│   │   ├── memory/           # State, context & memory managers
│   │   └── verification/     # Self-reflection & validation checks
│   └── utils/                # Logging, formatting, telemetry helpers
├── eval/                     # Benchmark suite & evaluation datasets
│   ├── test_cases/           # >= 10 realistic test cases (including hard edge cases)
│   ├── run_eval.py / .ts     # Automated benchmark comparing baseline vs solution
│   └── results/              # JSON / CSV outputs of benchmark runs
├── trajectories/             # Exported agent execution traces (JSON/Markdown)
└── tests/                    # Unit and integration tests
```

---

## 5. Experimentation & Changelog Standard

For every meaningful change you introduce, document the iteration using this schema in `CHANGELOG.md`:

```markdown
### Iteration [X]: [Brief Title of Change]
- **Hypothesis:** Why are we making this change? (e.g., "Adding a AST-parser tool will reduce syntax hallucination in code review by 40%").
- **Implementation:** Which files were changed and what agent components were added/modified (tools, memory, verification loop, orchestration)?
- **Evaluation Evidence:**
  - Baseline Metric: [Score / Latency / Cost]
  - Iteration [X] Metric: [Score / Latency / Cost]
  - Observed Delta: [e.g., +28% accuracy, -$0.02 cost]
- **Decision:** [Kept / Revised / Removed]
- **Key Insight / Learning:** What failure mode was exposed or what did this experiment prove?
```

---

## 6. The 4 Submission Deliverables Checklist

Every model working on this project must ensure the final deliverables are complete and aligned with micro1 requirements:

- [ ] **Deliverable 1: Solution Code + README + Changelog**
  - Identifies target user & specific bottleneck.
  - Explains why agentic capabilities (tools, memory, reflection) were necessary.
  - Contains complete `CHANGELOG.md` showing progression.
  - Concludes with the **Main Failure Mode** and your contrarian **Hot Take**.
- [ ] **Deliverable 2: Reproduction Guide (`REPRODUCTION.md`)**
  - Step-by-step instructions from a blank machine.
  - Single command to run the baseline evaluation.
  - Single command to run the advanced agent solution.
  - Hardware requirements, versions, approximate runtime, and API cost estimate.
- [ ] **Deliverable 3: Solution Video Script**
  - Timed outline for a $\le 5$ minute demo video.
  - Flow: Problem $\rightarrow$ Baseline Failure $\rightarrow$ Agent Solution $\rightarrow$ Benchmark Proof $\rightarrow$ What was removed.
- [ ] **Deliverable 4: Agent Trajectories (`trajectories/`)**
  - Clean trace logs showing: User Input $\rightarrow$ Agent Thought $\rightarrow$ Tool Call $\rightarrow$ Tool Output $\rightarrow$ Verification Step $\rightarrow$ Final Output.

---

## 7. Ground Rules & Safety Guardrails (From micro1 Rule Book)

1. **Pre-existing vs Added Code:** Clearly indicate if third-party libraries or starter code are used; clearly delineate what we built during the hackathon.
2. **Tool Licensing:** Only use open-source or properly licensed APIs and libraries.
3. **Sandbox Consequential Actions:** Never allow an agent to run destructive shell commands or external writes without safety barriers or user simulation flags.
4. **Human in the Loop:** Any decision that impacts real users (e.g., hiring, medical, financial) must present evidence to a human reviewer rather than making unilateral final decisions.
5. **Data Privacy:** Use public or synthetic datasets. Never commit real private information, credentials, or API keys.
