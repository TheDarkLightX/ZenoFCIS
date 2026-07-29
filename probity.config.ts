import {
  defineConfig,
  forbidCommandPattern,
  requireCommand,
  type Rule,
} from '@nizos/probity'

const rustCommandPolicy: Rule = (action) => {
  if (action.kind !== 'command') return { kind: 'pass' }

  const normalized = action.command.replace(/\\\r?\n/g, ' ')
  const segments = normalized.split(/(?:&&|\|\||[;\n])/)
  for (const segment of segments) {
    const cargo = segment.match(
      /(?:^|\s)cargo\s+(\+\S+\s+)?(build|check|clippy|doc|run|test)\b(.*)$/,
    )
    if (!cargo) continue
    if (cargo[1]?.trim() !== '+1.97.1') {
      return {
        kind: 'violation',
        reason: 'Use the pinned Rust toolchain: `cargo +1.97.1 ...`.',
      }
    }
    if (!/(?:^|\s)--locked(?:\s|$)/.test(cargo[3] ?? '')) {
      return {
        kind: 'violation',
        reason: 'Use `--locked` for build, check, Clippy, doc, run, and test commands.',
      }
    }
  }
  return { kind: 'pass' }
}

export default defineConfig({
  rules: [
    forbidCommandPattern({
      match: /\bgit\s+reset\s+--hard\b/,
      reason: 'Do not discard repository state with `git reset --hard`.',
    }),
    forbidCommandPattern({
      match: /\bgit\s+clean\s+-[^\s]*f/,
      reason: 'Do not remove untracked repository state with `git clean -f`.',
    }),
    forbidCommandPattern({
      match: /\bgit\s+(?:checkout\s+--|restore\s+--source)\b/,
      reason: 'Use a reviewed, recoverable edit instead of discarding file state.',
    }),
    forbidCommandPattern({
      match: /\bcargo\s+(?:\+\S+\s+)?update\b/,
      reason: 'Do not mutate the locked dependency graph during ordinary development.',
    }),
    forbidCommandPattern({
      match: /\bcargo\s+(?:\+\S+\s+)?publish\b/,
      reason: 'Crate publication is an owner release action, not an agent development step.',
    }),
    forbidCommandPattern({
      match: /\bgit\s+tag\b/,
      reason: 'Signed release tagging requires the owner release checklist.',
    }),
    forbidCommandPattern({
      match: /\bgh\s+pr\s+merge\b/,
      reason: 'PR merging requires explicit owner instruction after exact-head review.',
    }),
    forbidCommandPattern({
      match: /\bgh\s+release\s+create\b/,
      reason: 'GitHub release creation requires the signed owner release ceremony.',
    }),
    forbidCommandPattern({
      match: /\bnpm\s+install\b/,
      reason: 'Use the exact locked development graph with `npm ci --ignore-scripts`.',
    }),
    rustCommandPolicy,
    requireCommand({
      before: { kind: 'command', match: /\bgit\s+commit\b/ },
      command: /python3\s+tools\/atdd\.py\s+run\s+--all/,
      reason: 'Run `python3 tools/atdd.py run --all` immediately before committing.',
    }),
  ],
})
