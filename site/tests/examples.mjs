// Shared by the per-template example readers under tests/templates/: the
// line format of tests/decision-examples.txt, and the outcome names.

export const KINDS = { accept: "Accept", reject: "Reject", failure: "CommittedFailure" };

export function fail(message) {
  throw new Error(message);
}

// The examples, without comments and blank lines, each split at its `|`
// separators into trimmed parts.
export function lines(text) {
  return text
    .split("\n")
    .map((line) => line.trim())
    .filter((line) => line && !line.startsWith("#"))
    .map((line) => ({ line, parts: line.split("|").map((part) => part.trim().split(/\s+/)) }));
}

export function integer(token, line) {
  const value = Number(token);
  if (!Number.isInteger(value)) fail(`not an integer: ${token}: ${line}`);
  return value;
}

export function named(table, token, line) {
  return table[token] ?? fail(`unknown ID ${token}: ${line}`);
}

export function kind(token, line) {
  return KINDS[token] ?? fail(`unknown outcome ${token}: ${line}`);
}

export function reason(token) {
  return token === "-" ? null : Number(token);
}
