import { writeFileSync } from "node:fs";
import { points } from "./drawing-data.mjs";
const escapeXml = (s) => String(s).replaceAll("&", "&amp;").replaceAll("<", "&lt;").replaceAll(">", "&gt;");
export function poster(story, out) {
  const m = story.walk,
    n = m.n;
  const theme = {
    bg: ["#fafafa", "#0a0a0a"],
    ink: ["#181c19", "#f0f1ec"],
    sub: ["#535e56", "#bac1b9"],
    edge: ["#9ba79e", "#64726a"],
    rule: ["#d1d7d2", "#3d4841"],
    green: ["#126747", "#9adeb5"],
    fill: ["#dbece1", "#1c3829"],
  };
  const css = Object.entries(theme)
    .map(([k, v]) => ".s-" + k + "{stroke:" + v[1] + "}.f-" + k + "{fill:" + v[1] + "}")
    .join("");
  const color = (k) => theme[k][0];
  const a = [
    '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 1200 730" width="1200" height="730" role="img" aria-labelledby="t d">',
    '<title id="t">' +
      escapeXml(story.title) +
      '</title><desc id="d">A binary tree of move histories folds into a graph of endpoints. Highlighted histories reach the goal. The labels give their turn counts.</desc>',
    "<style>text{font-family:Inter,Arial,sans-serif}.mono{font-family:Consolas,monospace}@media(prefers-color-scheme:dark){" +
      css +
      "}</style>",
    '<rect width="1200" height="730" class="f-bg" fill="' + color("bg") + '"/>',
  ];
  function label(x, y, s, size, k = "ink", bold = false, mono = false) {
    a.push(
      '<text x="' +
        x +
        '" y="' +
        y +
        '" class="f-' +
        k +
        (mono ? " mono" : "") +
        '" fill="' +
        color(k) +
        '" font-size="' +
        size +
        '" font-weight="' +
        (bold ? 700 : 400) +
        '">' +
        escapeXml(s) +
        "</text>",
    );
  }
  function edge(x, y, X, Y, k = "edge", w = 1.4) {
    a.push(
      '<path d="M' +
        x +
        " " +
        y +
        " L" +
        X +
        " " +
        Y +
        '" fill="none" stroke="' +
        color(k) +
        '" class="s-' +
        k +
        '" stroke-width="' +
        w +
        '" stroke-linecap="round"/>',
    );
  }
  function disk(x, y, r, k = "bg", stroke = "edge", w = 1.2) {
    a.push(
      '<circle cx="' +
        x +
        '" cy="' +
        y +
        '" r="' +
        r +
        '" fill="' +
        color(k) +
        '" stroke="' +
        color(stroke) +
        '" class="f-' +
        k +
        " s-" +
        stroke +
        '" stroke-width="' +
        w +
        '"/>',
    );
  }
  const numWord = (n) =>
    ["Zero", "One", "Two", "Three", "Four", "Five", "Six", "Seven", "Eight", "Nine", "Ten"][n] ?? String(n);
  label(58, 76, story.title, Math.min(48, 1500 / story.title.length), "ink", true);
  label(
    60,
    119,
    "Reach a square " +
      m.goal.right +
      " steps right and " +
      m.goal.up +
      " steps up in exactly " +
      n +
      " moves.",
    23,
    "sub",
  );
  edge(60, 151, 1140, 151, "rule");
  label(61, 204, m.routes.length + " possible routes", 30, "ink", true);
  label(666, 204, n + 1 + " possible endpoints", 30, "ink", true);
  const yy = (u, d) => 378 + (u - d / 2) * 39;
  function point(d, i, merged) {
    const u = m.coordinates[m.states[m.layers[d][i]]][1];
    return merged ? [671 + (d * 355) / n, yy(u, d)] : [67 + (385 * d) / n, 243 + (270 * (i + 0.5)) / 2 ** d];
  }
  for (const merged of [false, true]) {
    const seen = new Set(),
      seenNodes = new Set();
    for (let d = 1; d <= n; d++)
      for (let i = 0; i < 2 ** d; i++) {
        const A = point(d - 1, i >> 1, merged),
          B = point(d, i, merged),
          key = A + ">" + B;
        if (seen.has(key)) continue;
        seen.add(key);
        edge(...A, ...B);
      }
    for (const name of [m.mostTurns, m.fewestTurns]) {
      let i = 0;
      for (let d = 1; d <= n; d++) {
        const start = point(d - 1, i, merged);
        i = 2 * i + (name[d - 1] === "U");
        edge(...start, ...point(d, i, merged), "green", 2.6);
      }
    }
    for (let d = 0; d <= n; d++)
      for (let i = 0; i < 2 ** d; i++) {
        const pt = point(d, i, merged),
          key = String(pt);
        if (seenNodes.has(key)) continue;
        seenNodes.add(key);
        const leaf = d === n,
          sel =
            leaf &&
            (merged
              ? m.solutions.includes(m.layers[d][i])
              : [m.fewestTurns, m.mostTurns].includes(m.layers[d][i]));
        disk(...pt, sel ? 5 : leaf ? 2.6 : 2.8, sel ? "green" : "bg", sel ? "green" : "edge");
      }
  }
  for (let u = 0; u <= n; u++) {
    const active = u === m.goal.up;
    label(
      1044,
      yy(u, n) + 6,
      "×" + m.groups[u] + (active ? "  goal" : ""),
      active ? 20 : 18,
      active ? "green" : "sub",
      active,
    );
  }
  edge(497, 371, 586, 371, "sub", 2);
  edge(577, 362, 586, 371, "sub", 2);
  edge(577, 380, 586, 371, "sub", 2);
  label(61, 555, "Each branch adds R or U.", 20, "sub");
  label(666, 555, numWord(m.solutions.length) + " routes reach the goal.", 20, "green", true);
  edge(60, 590, 1140, 590, "rule");
  label(61, 640, "Selection adds a cost.", 27, "ink", true);
  label(61, 682, "Count direction changes along each history.", 20, "sub");
  function mini(s, x, y) {
    const pts = points(m, s).map(([i, j]) => [x + i * 21, y - j * 14]);
    for (let i = 1; i < pts.length; i++) edge(...pts[i - 1], ...pts[i], "green", 3);
    disk(...pts.at(-1), 3.2, "green", "green");
    const count = m.costs[s];
    label(x, y + 33, count + (count === 1 ? " turn" : " turns"), 17, "sub");
  }
  mini(m.mostTurns, 770, 654);
  mini(m.fewestTurns, 965, 654);
  a.push("</svg>");
  writeFileSync(out, a.join("\n"));
}
