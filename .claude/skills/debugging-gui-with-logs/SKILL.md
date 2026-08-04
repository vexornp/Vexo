---
name: debugging-gui-with-logs
description: Use when debugging GUI bugs in the Vexo desktop demo (or iOS app). Enforces a log-evidence-first workflow: instrument with log::debug! using a unique prefix, hand the user a tee-to-file run command, the user runs the demo and reproduces the bug, then you read the captured log file and fix the root cause. Never run the demo yourself, never reason about a GUI bug without log evidence first.
---

# Debugging GUI With Logs

## Overview

GUI bugs live in a widget tree, a render pipeline, an event loop, and a layout engine running together across many frames. Reasoning about them from source alone produces confident, wrong fixes.

**Core principle:** Log evidence FIRST. Theory SECOND.

**Violating the letter of this workflow is violating the spirit of debugging.**

## The Iron Law

```
NO GUI THEORIZING WITHOUT LOG EVIDENCE FIRST
NO RUNNING THE DEMO YOURSELF — EVER
```

You cannot interact with the GUI, and your terminal may be on a different display (e.g. non-Retina) producing misleading render results. The user runs the demo; you instrument and read logs.

## When to Use

Use for ANY visual or behavioral bug in the desktop demo (or iOS app via logs):

- Wrong layout / overflow / misalignment
- Widget not appearing, disappearing, flickering
- Gestures, focus, cursor, text input misbehaving
- Navigation stalls, animation glitches
- Render output looks wrong (color, clipping, position)

**Use ESPECIALLY when:**

- The bug only appears in a complex widget tree (see "Isolate Before Theorizing" below)
- You're tempted to "just reason it out" from the code
- A previous fix attempt didn't work

## The Workflow

Complete each step before proceeding to the next. Never skip to theory.

### Step 1: Form a Hypothesis

State, explicitly and in writing:

- What you think is happening
- Where in the pipeline you think it breaks (widget? element? render object? layout? event?)
- What signal in the logs would CONFIRM or REFUTE this hypothesis

If you can't name a confirming/refuting signal, your hypothesis isn't testable yet. Don't move on.

### Step 2: Instrument with `log::debug!` and a Unique Prefix

Add `log::debug!` calls at the boundaries relevant to your hypothesis.

**Always use a unique, greppable prefix** for this debugging session, e.g.:

```rust
log::debug!("[DBG-NAV] chat_screen rebuild, stack_depth={}", depth);
log::debug!("[DBG-NAV] layout result: {:?}", node_rect);
log::debug!("[DBG-NAV] render_retain: dirty={}, animating={}", dirty, animating);
```

Rules:

- One prefix per investigation (e.g. `[DBG-NAV]`, `[DBG-KB]`, `[DBG-LAYOUT]`)
- Log at EVERY component boundary the hypothesis touches: data in, data out, branch taken
- Log the values that would distinguish your hypothesis from alternatives — not just "reached here"
- If the bug is in a hot path (scroll, keyboard animation), log frame counters so you can see the sequence across frames

### Step 3: Hand the User a tee-to-file Run Command

**You do NOT run the demo.** You give the user a command that:

1. Sets `RUST_LOG=debug`
2. Pipes through `grep` for your unique prefix (so the live terminal shows only relevant lines)
3. `tee`s the filtered output to a tmp file that YOU will read afterward

**Canonical command template:**

```bash
RUST_LOG=debug cargo run -p desktop_demo 2>&1 | grep "<PREFIX>" | tee /tmp/vexo_debug.log
```

**Concrete example:**

```bash
RUST_LOG=debug cargo run -p desktop_demo 2>&1 | grep "\[DBG-NAV\]" | tee /tmp/vexo_debug.log
```

Then tell the user explicitly:

- "Run this command."
- "Once the window opens, reproduce the bug: \<exact reproduction steps\>."
- "When you've reproduced it, close the window / Ctrl-C and tell me you're done."

Notes on the command:

- `env_logger` writes to stderr, so `2>&1` is required before the grep
- If you need surrounding context lines, use `grep -A 2 -B 2 "<PREFIX>"` instead of a bare grep
- If you want the FULL log captured (not just your prefix) so you can grep freely when reading, drop the grep and `tee` everything: `RUST_LOG=debug cargo run -p desktop_demo 2>&1 | tee /tmp/vexo_debug.log` — then grep with the Read/Grep tools when reading
- For iOS, adapt to however the iOS app forwards logs; the principle (capture to a file you can read back) is the same

### Step 4: Read the Log Evidence

Once the user confirms they've reproduced and stopped the app, READ `/tmp/vexo_debug.log` yourself using the Read or Grep tool.

- Look for the confirming/refuting signal you named in Step 1
- Trace the sequence across frames — did your boundary values match the hypothesis?
- If the log is empty, your prefix didn't match or the code path wasn't hit. That's evidence too — go back to Step 1 with a new hypothesis (the path isn't where you thought)

You have no evidence until you have actually read the file. Do not theorize about the result; read it.

### Step 5: Fix the Root Cause

Only now, with log evidence in hand:

- Fix the actual root cause the evidence points to
- If you're changing control flow in a hot path, write the integration test FIRST (see "Development Workflow" in CLAUDE.md)
- Remove the throwaway `log::debug!` lines, or keep the genuinely useful ones
- Ask the user to re-run and verify the fix

## Isolate Before Theorizing

When a bug only appears in a complex widget tree, strip the tree to the minimum repro FIRST (e.g. a single widget in a bare `MultiChild`). This narrows the search space dramatically before forming hypotheses.

"Works alone, breaks in a tree" immediately points at the surrounding flex chain, not the widget itself.

Do this before Step 1 if the bug is embedded in a large subtree.

## Red Flags — STOP

If you catch yourself:

- Theorizing about a GUI bug before adding any `log::debug!`
- Running `cargo run -p desktop_demo` yourself (or `cargo run | grep` — that STILL launches the GUI on your display)
- Rationalizing "I just need to see if it compiles and runs" by executing the demo
- Proposing a fix without a log line that confirms the root cause
- Saying "it's probably X" with no evidence in hand

**STOP. Go back to Step 1.**

## Rationalization Prevention

| Excuse | Reality |
|--------|---------|
| "I can reason about this from the code" | GUI bugs span widget+element+render+layout across frames. You can't. Get logs. |
| "I'll just run it quickly to check" | You can't interact with the GUI, and your display may mislead you. The user runs it. |
| "`cargo run \| grep` doesn't really show me the GUI" | It STILL launches the GUI on your display. Forbidden. Hand the command to the user. |
| "The fix is obvious, no need to instrument" | "Obvious" GUI fixes are wrong most of the time. Instrument first. |
| "I added a log, I don't need to read the file" | If you didn't read `/tmp/vexo_debug.log`, you have no evidence. Read it. |
| "Previous run is enough" | Each bug needs its own evidence. Re-instrument, re-run, re-read. |

## Related

- **systematic-debugging** — the general four-phase debugging process; this skill is the GUI-specific enforcement layer
- **CLAUDE.md "Development Workflow"** — never run the demo yourself; write integration tests for hot-path control-flow changes
