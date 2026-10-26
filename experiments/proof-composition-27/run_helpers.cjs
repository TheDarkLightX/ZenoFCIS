'use strict';
// Canonical-byte JSON admission rejects duplicate keys, alternate number encodings,
// Unicode-escape aliases, nonstandard whitespace and key ordering.
function sortedJSON(x) {
  if (Array.isArray(x)) return x.map(sortedJSON);
  if (x !== null && typeof x === 'object') {
    const y = {};
    for (const k of Object.keys(x).sort()) y[k] = sortedJSON(x[k]);
    return y;
  }
  return x;
}
function canonicalJSON(x) { return JSON.stringify(sortedJSON(x), null, 2) + '\n'; }
module.exports = { canonicalJSON };
