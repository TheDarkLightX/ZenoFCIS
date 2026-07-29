# Deterministic coding-agent guardrails

ZenoFCIS provides an optional repository configuration for Probity `1.10.0`.
It evaluates coding-agent file writes and shell commands before they execute.
The configuration is development tooling and never enters protocol, proof,
runtime, or production authority.

## Pinned environment

```text
Node:   22.23.1
Probity: 1.10.0
```

Install the exact locked development graph:

```bash
npm ci --ignore-scripts
python3 tools/check_probity.py
```

The self-test sends hostile and permitted Codex hook payloads through the exact
pinned Probity binary. It verifies fail-closed parsing and the configured
command rules.

## Enforced rules

The repository configuration is deliberately deterministic. It:

- rejects destructive Git reset, clean, checkout, and restore commands;
- rejects dependency graph mutation through `cargo update`;
- rejects direct `cargo publish`, Git tagging, GitHub release creation, and PR
  merging from an ordinary coding-agent session;
- requires pinned Rust `+1.97.1` and `--locked` for build, check, Clippy,
  documentation, run, and test commands;
- requires `python3 tools/atdd.py run --all` as the immediately preceding
  canonical session event before `git commit`.

Probity's AI-judged `enforceTdd` rule is not enabled. Its decision would depend
on an external model and its deterministic fast path does not currently support
Rust. ZenoFCIS instead uses reviewable BDD features, the closed ATDD registry,
ordinary Rust unit/property/refinement tests, and repository CI.

## Codex hook setup

Probity's Codex integration currently requires user-level hook activation. Add
the following to `~/.codex/config.toml`:

```toml
[features]
codex_hooks = true
```

Then add this `PreToolUse` entry to `~/.codex/hooks.json`:

```json
{
  "hooks": {
    "PreToolUse": [
      {
        "matcher": "^(Bash|apply_patch|Edit|Write)$",
        "hooks": [
          {
            "type": "command",
            "command": "npx --no-install probity --agent codex"
          }
        ]
      }
    ]
  }
}
```

Hook installation changes user-level Codex configuration and is therefore
opt-in. The repository does not modify it automatically.

## Nonclaims

Probity prevents selected development actions before execution. It is not a
formal checker, security boundary, CI replacement, release signer, or evidence
that a transition satisfies FCIS laws. A disabled, missing, misconfigured, or
compromised hook cannot weaken the independently enforced repository gates.
