import { points } from "./drawing-data.mjs";

export const palettes = {
  light: {
    bg: "#fafafa",
    ink: "#161a18",
    sub: "#535d58",
    grid: "#c6ccc7",
    edge: "#79857e",
    green: "#126747",
    greenbg: "#d8ebe0",
    warm: "#98512b",
    warmbg: "#f0e2d6",
    neutral: "#eef0ed",
  },
  dark: {
    bg: "#0a0a0a",
    ink: "#f1f1eb",
    sub: "#b7bdb7",
    grid: "#424b45",
    edge: "#7e8d82",
    green: "#9adeb5",
    greenbg: "#193627",
    warm: "#ecb592",
    warmbg: "#3f2b20",
    neutral: "#191e1a",
  },
};
const clamp = (x) => Math.max(0, Math.min(1, x)),
  mix = (a, b, p) => a + (b - a) * p;
const smooth = (x) => ((x = clamp(x)), x * x * (3 - 2 * x)),
  on = (x, a, b) => smooth((x - a) / (b - a));

export function painter(c, story, theme) {
  const C = palettes[theme],
    { walk: W, transfer: T, bytes: B } = story;
  function alpha(a, fn) {
    if (a < 0.003) return;
    c.save();
    c.globalAlpha *= Math.min(a, 1);
    fn();
    c.restore();
  }
  function txt(s, x, y, size = 24, col = C.ink, font = "Face", align = "left") {
    c.font = size + "px " + font;
    c.fillStyle = col;
    c.textAlign = align;
    c.textBaseline = "middle";
    c.fillText(String(s), x, y);
  }
  function block(str, x, y, size, col, max, spacing, face = "Face") {
    // Keep long authored captions inside the two-line subtitle area.
    let rows = [];
    for (; size >= 17; size--) {
      c.font = size + "px " + face;
      rows = [""];
      for (const word of str.split(/\s+/)) {
        const i = rows.length - 1,
          next = rows[i] ? rows[i] + " " + word : word;
        if (rows[i] && c.measureText(next).width > max) rows.push(word);
        else rows[i] = next;
      }
      if (rows.length <= 2 && rows.every((row) => c.measureText(row).width <= max)) break;
    }
    rows.forEach((row, i) => {
      c.font = size + "px " + face;
      c.fillStyle = col;
      c.textAlign = "left";
      c.textBaseline = "middle";
      c.fillText(row, x, y + i * spacing, max);
    });
  }
  function line(x, y, X, Y, col = C.grid, lw = 1.5) {
    c.strokeStyle = col;
    c.lineWidth = lw;
    c.beginPath();
    c.moveTo(x, y);
    c.lineTo(X, Y);
    c.stroke();
  }
  function dot(x, y, r, fill, stroke = null, lw = 1.5) {
    c.beginPath();
    c.arc(x, y, r, 0, Math.PI * 2);
    if (fill) {
      c.fillStyle = fill;
      c.fill();
    }
    if (stroke) {
      c.lineWidth = lw;
      c.strokeStyle = stroke;
      c.stroke();
    }
  }
  function square(x, y, w, h, fill, edge = null, lw = 1.5) {
    if (fill) {
      c.fillStyle = fill;
      c.fillRect(x, y, w, h);
    }
    if (edge) {
      c.lineWidth = lw;
      c.strokeStyle = edge;
      c.strokeRect(x, y, w, h);
    }
  }
  function cross(x, y, r, col) {
    line(x - r, y - r, x + r, y + r, col, 2);
    line(x - r, y + r, x + r, y - r, col, 2);
  }
  function ring(x, y) {
    dot(x, y, 13, C.bg, C.green, 2.5);
    dot(x, y, 6, C.green);
  }
  function trace(points, p, col, width = 4, marker = true) {
    if (p <= 0 || points.length < 2) return;
    const f = (points.length - 1) * clamp(p),
      i = Math.min(Math.floor(f), points.length - 2);
    const q = [mix(points[i][0], points[i + 1][0], f - i), mix(points[i][1], points[i + 1][1], f - i)];
    c.beginPath();
    c.moveTo(...points[0]);
    for (let k = 1; k <= i; k++) c.lineTo(...points[k]);
    c.lineTo(...q);
    c.strokeStyle = col;
    c.lineWidth = width;
    c.stroke();
    if (marker) dot(...q, 6, col, C.bg, 1.7);
  }
  function grid(x, y, unit, nx, ny, goal) {
    const map = ([i, j]) => [x + i * unit, y - j * unit];
    for (let i = 0; i <= nx; i++) line(...map([i, 0]), ...map([i, ny]), C.grid);
    for (let j = 0; j <= ny; j++) line(...map([0, j]), ...map([nx, j]), C.grid);
    if (goal) ring(...map(goal));
    dot(x, y, 5, C.ink);
    return map;
  }
  function routePlot(s, map, p, color = C.green) {
    trace(points(W, s).map(map), p, color, 4.5);
  }
  function bigValue(s, x, y, sz = 54, col = C.ink) {
    txt(s, x, y, sz, col, "Heavy");
  }
  const treeLo = 157,
    treeHi = 552,
    mid = (treeLo + treeHi) / 2,
    delta = (treeHi - treeLo) / (W.n + 1);
  const hx = (d) => 86 + (d * 700) / W.n;
  function trees(d, i, morph) {
    const paths = W.layers[d];
    const ups = W.coordinates[W.states[paths[i]]][1];
    return [
      hx(d),
      mix(treeLo + ((treeHi - treeLo) * (i + 0.5)) / paths.length, mid + (ups - d / 2) * delta, morph),
    ];
  }
  function drawTree(depth, morph, opts = {}) {
    const m = on(morph, 0.0, 1),
      hl = opts.hl || new Set(),
      pathCol = opts.color ?? C.green;
    for (let d = 1; d <= W.n; d++) {
      const pg = clamp(depth - d + 1);
      if (!pg) continue;
      // After coalescence, draw each coincident edge once.
      const drawn = new Set();
      for (let i = 0; i < W.layers[d].length; i++) {
        const a = trees(d - 1, Math.floor(i / 2), m),
          b = trees(d, i, m);
        const k = a.join(",") + ">" + b.join(",");
        if (m > 0.999 && drawn.has(k)) continue;
        drawn.add(k);
        line(...a, mix(a[0], b[0], pg), mix(a[1], b[1], pg), C.grid, m > 0.96 ? 2.25 : 1.5);
      }
    }
    if (hl.size) {
      for (const word of hl) {
        let idx = 0;
        for (let d = 0; d < W.n; d++) {
          const a = trees(d, idx, m);
          idx = idx * 2 + (word[d] === "U" ? 1 : 0);
          const b = trees(d + 1, idx, m);
          line(...a, ...b, W.solutions.includes(word) ? C.green : pathCol, 2.3);
        }
      }
    }
    for (let d = 0; d <= Math.floor(depth + 0.0001); d++) {
      const seen = new Set();
      for (let i = 0; i < W.layers[d].length; i++) {
        const p = trees(d, i, m);
        const key = Math.round(p[1] * 10);
        if (seen.has(key)) continue;
        seen.add(key);
        const lit = m > 0.995 && d === W.n && W.solutions.includes(W.layers[d][i]);
        const hit = d === W.n && hl.has(W.layers[d][i]);
        if (hit && W.solutions.includes(W.layers[d][i])) {
          ring(...p);
          continue;
        }
        if (hit && opts.reject) {
          cross(...p, 3.6, pathCol);
          continue;
        }
        dot(
          ...p,
          lit ? 7.5 : d === W.n ? 3.1 + 2 * m : 3.5,
          lit ? C.green : C.bg,
          lit ? C.green : C.edge,
          lit ? 2.3 : 1.4,
        );
      }
    }
  }
  function walk(p, shot) {
    const sx = 551,
      sy = 531,
      unit = Math.min(76, 650 / W.n, 355 / W.n);
    const map = grid(sx, sy, unit, W.n, W.n, [W.goal.right, W.goal.up]);
    const gp = map([W.goal.right, W.goal.up]);
    txt("Goal", gp[0] + 18, gp[1] - 16, 21, C.green, "Heavy");
    txt("Start", sx - 12, sy + 28, 19, C.sub);
    txt("Available moves", 74, 207, 23, C.sub);
    txt("R  →  right", 74, 260, 33, C.ink, "Code");
    txt("U  ↑  up", 74, 316, 33, C.ink, "Code");
    if (shot.path) {
      txt(shot.path, 74, 427, 44, C.ink, "Code");
      routePlot(shot.path, map, on(p, 0.08, 0.88));
      alpha(on(p, 0.82, 0.92), () => txt("Endpoint (" + W.states[shot.path] + ")", 75, 477, 23, C.sub));
      return;
    }
    const one = W.routes[0];
    const winner = W.fewestTurns;
    groupRoute(1 - on(p, 0.48, 0.64), one, on(p, 0.06, 0.4), C.warm);
    groupRoute(on(p, 0.48, 0.64), winner, on(p, 0.5, 0.86), C.green);
    function groupRoute(op, s, pr, col) {
      alpha(op, () => {
        txt(s, 74, 427, 44, col, "Code");
        routePlot(s, map, pr, col);
      });
    }
    alpha(on(p, 0.86, 0.93), () => txt("Reaches the goal", 75, 477, 23, C.green));
  }
  function tree(p) {
    const depth = W.n * on(p, 0.04, 0.78);
    drawTree(depth, 0);
    const n = Math.floor(depth + 0.0001);
    bigValue(String(W.layers[n].length), 873, 276, 83);
    txt(n === 1 ? "possible prefixes" : "possible paths", 875, 344, 25, C.sub);
    txt("after " + n + (n === 1 ? " move" : " moves"), 875, 384, 23, C.sub);
    alpha(on(p, 0.76, 0.91), () => txt("2 choices per move", 875, 470, 22, C.ink));
    for (let d = 0; d <= W.n; d++)
      alpha(clamp(depth + 1 - d), () => txt(String(d), hx(d), 572, 16, C.sub, "Code", "center"));
  }
  function budget(p) {
    const n = Math.min(story.budget, Math.floor(clamp((p - 0.03) / 0.57) * story.budget + 1e-6)),
      runs = W.routes.slice(0, n);
    drawTree(W.n, 0, { hl: new Set(runs), color: C.warm, reject: true });
    txt("Trials used", 870, 208, 22, C.sub);
    bigValue(n + " / " + story.budget, 866, 260, 54);
    for (let i = 0; i < story.budget; i++) {
      const y = 340 + i * 41;
      txt(W.routes[i], 870, y, 24, i < n ? C.ink : C.edge, "Code");
      if (i < n) {
        if (W.solutions.includes(W.routes[i])) ring(1057, y);
        else cross(1057, y, 6, C.warm);
      }
    }
    alpha(on(p, 0.64, 0.77), () => {
      txt("Budget exhausted", 870, 489, 25, C.warm, "Heavy");
      txt(W.routes.length - story.budget + " routes untested", 870, 531, 21, C.sub);
    });
  }
  function merge(p) {
    const m = on(p, 0.1, 0.64);
    drawTree(W.n, m);
    const focus = on(p, 0.08, 0.22) * (1 - on(p, 0.82, 0.95));
    alpha(focus, () => {
      for (const seq of [W.fewestTurns, W.mostTurns]) {
        let i = 0;
        const pts = [trees(0, 0, m)];
        for (let d = 0; d < W.n; d++) {
          i = 2 * i + (seq[d] === "U" ? 1 : 0);
          pts.push(trees(d + 1, i, m));
        }
        trace(pts, on(p, 0.07, 0.35), C.green, 2.6);
      }
    });
    alpha(1 - on(p, 0.09, 0.28), () => {
      bigValue(W.routes.length, 881, 270, 78);
      txt("possible routes", 884, 337, 24, C.sub);
    });
    alpha(on(p, 0.48, 0.72), () => {
      bigValue(W.groups.length, 884, 246, 70);
      txt("distinct endpoints", 884, 302, 23, C.sub);
      for (let u = 0; u < W.groups.length; u++)
        txt(
          "×" + W.groups[u],
          hx(W.n) + 15,
          mid + (u - W.n / 2) * delta,
          19,
          u === W.goal.up ? C.green : C.sub,
          "Code",
        );
      line(884, 333, 1166, 333, C.grid);
      txt("Goal   " + W.goal.right + "R + " + W.goal.up + "U", 884, 369, 20, C.sub, "Code");
      bigValue(W.solutions.length, 884, 423, 55, C.green);
      txt("routes reach the goal", 884, 466, 24, C.green);
    });
  }
  function select(p) {
    const compared = [...new Set([W.mostTurns, ...W.optima])];
    compared.forEach((route, i) => {
      const x = 100 + i * 380,
        y = 438,
        step = Math.min(70, 270 / W.goal.right, 180 / W.goal.up);
      const map = grid(x, y, step, W.goal.right, W.goal.up, [W.goal.right, W.goal.up]);
      const best = W.optima.includes(route),
        color = best ? C.green : C.sub,
        count = W.costs[route];
      txt(route, x, 185, 32, C.ink, "Code");
      routePlot(route, map, on(p, 0.05 + i * 0.08, 0.59 + i * 0.08), color);
      alpha(on(p, 0.64, 0.83), () => {
        txt(count + (count === 1 ? " turn" : " turns"), x, 491, 32, color, "Heavy");
        if (best) txt("Minimum", x, 535, 21, color);
      });
    });
  }
  function language(p) {
    [W.routes[0], W.fewestTurns].forEach((route, i) => {
      const x = i ? 730 : 164,
        step = Math.min(90, 420 / W.n, 200 / W.goal.up);
      const map = grid(x, 444, step, W.n, W.goal.up, [W.goal.right, W.goal.up]);
      txt(i ? "Right or up" : "Right only", x, 189, 28, C.ink, "Heavy");
      routePlot(route, map, on(p, i ? 0.21 : 0.05, i ? 0.72 : 0.57), i ? C.green : C.warm);
      alpha(on(p, 0.66, 0.81), () =>
        txt(i ? "Goal reached" : "Goal unreachable", x, 514, 31, i ? C.green : C.warm, "Heavy"),
      );
    });
  }
  function programs(p) {
    const f = on(p, 0.1, 0.73),
      tested = Math.floor(256 * f + 0.000001);
    const rowX = 131,
      x0 = 266,
      dx = 133,
      y0 = 261,
      dy = 93;
    txt("Upper bound", x0 + dx, 159, 21, C.sub, "Face", "center");
    txt("Lower", rowX, 199, 19, C.sub, "Face", "center");
    B.uppers.forEach((h, j) => txt(h, x0 + j * dx, 200, 22, C.ink, "Code", "center"));
    B.lowers.forEach((l, i) => txt(l, rowX, y0 + i * dy, 22, C.ink, "Code", "center"));
    let alive = 0;
    for (const a of B.candidates) {
      const px = x0 + B.uppers.indexOf(a.h) * dx,
        py = y0 + B.lowers.indexOf(a.l) * dy;
      const failed = a.misses.some((b) => b < tested),
        win = tested === 256 && !failed;
      if (!failed) alive++;
      square(
        px - 52,
        py - 33,
        104,
        66,
        failed ? C.neutral : win ? C.greenbg : C.bg,
        failed ? C.grid : win ? C.green : C.edge,
        win ? 2.5 : 1.4,
      );
      if (failed) cross(px, py, 8, C.warm);
      else if (win) {
        line(px - 10, py, px - 3, py + 7, C.green, 3);
        line(px - 3, py + 7, px + 11, py - 10, C.green, 3);
      } else dot(px, py, 5, null, C.edge, 1.5);
    }
    txt("Rule to match", 723, 188, 23, C.sub);
    txt(B.lo + " ≤ b < " + B.hi, 720, 243, 38, C.ink, "Code");
    txt("Byte inputs checked per program", 724, 316, 22, C.sub);
    line(725, 368, 1145, 368, C.grid, 4);
    line(725, 368, 725 + 420 * f, 368, C.green, 4);
    dot(725 + 420 * f, 368, 6, C.green);
    txt(tested + " / 256", 724, 414, 28, C.ink, "Code");
    txt(
      alive + (alive === 1 ? " candidate remains" : " candidates remain"),
      724,
      481,
      26,
      tested === 256 ? C.green : C.sub,
      "Heavy",
    );
    alpha(on(p, 0.75, 0.87), () =>
      txt((B.candidates.length * 256).toLocaleString("en-US") + " finite checks", 724, 526, 21, C.sub),
    );
    txt("Each cell is one interval program.", 131, 532, 20, C.sub);
  }
  function reuse(p) {
    const x = 634,
      y = 505,
      r = T.goal.right,
      u = T.goal.up,
      step = Math.min(87, 500 / r, 285 / u);
    const map = grid(x, y, step, r, u, [r, u]);
    txt("New goal", 73, 194, 23, C.sub);
    bigValue(r + " right, " + u + " up", 71, 259, 52);
    const route = T.events;
    txt(route, 73, 354, 37, C.green, "Code");
    trace(T.points.map(map), on(p, 0.15, 0.79), C.green, 4.5);
    alpha(on(p, 0.7, 0.86), () => {
      txt("One constructed solution", 75, 432, 26, C.ink, "Heavy");
      txt(T.possibilities + " possible move strings", 75, 476, 22, C.sub);
    });
  }
  function relation(p) {
    const x1 = 83,
      x2 = 394,
      x3 = 744,
      rows = [
        ["Route finding", "Routes", "Reach the goal legally"],
        ["Theorem proving", "Proofs", "Derive the stated claim"],
        ["Program synthesis", "Programs", "Meet the specification"],
      ];
    txt("Problem", x1, 172, 22, C.sub);
    txt("Possible answers  Y", x2, 172, 22, C.sub);
    txt("Condition  R(x, y)", x3, 172, 22, C.sub);
    for (let i = 0; i < 3; i++) {
      let y = 239 + i * 70;
      alpha(on(p, 0.01 + i * 0.12, 0.13 + i * 0.12), () => {
        line(x1, y - 35, 1197, y - 35, C.grid);
        txt(rows[i][0], x1, y, 27, C.ink, "Heavy");
        txt(rows[i][1], x2, y, 27);
        txt(rows[i][2], x3, y, 27);
      });
    }
    alpha(on(p, 0.38, 0.61), () => {
      txt("S(x) = { y ∈ Y | R(x, y) }", 640, 493, 40, C.green, "Code", "center");
    });
  }
  const scenes = { trace: walk, branch: tree, budget, merge, select, language, programs, reuse, relation };
  return (shot, p) => {
    c.setTransform(1.25, 0, 0, 1.25, 0, 0);
    c.fillStyle = C.bg;
    c.fillRect(0, 0, 1280, 720);
    c.lineCap = "round";
    c.lineJoin = "round";
    // The tree stays on screen through three views, so the audience can track each branch.
    const continuity = ["branch", "budget", "merge"].includes(shot.view);
    const op = clamp(Math.min((p * shot.seconds) / 0.32, ((1 - p) * shot.seconds) / 0.32));
    const first = shot === story.shots[0],
      last = shot === story.shots.at(-1);
    const titleOp = Math.min(
      first ? 1 : clamp((p * shot.seconds) / 0.28),
      last ? 1 : clamp(((1 - p) * shot.seconds) / 0.28),
    );
    alpha(titleOp, () => {
      c.font = "45px Heavy";
      const titleSize = Math.min(45, (45 * 1145) / c.measureText(shot.title).width);
      txt(shot.title, 72, 80, titleSize, C.ink, "Heavy");
    });
    line(73, 124, 1205, 124, C.grid, 1);
    alpha(continuity ? 1 : first || last ? titleOp : op, () => scenes[shot.view](p, shot));
    line(73, 597, 1205, 597, C.grid, 1);
    alpha(titleOp, () => block(shot.caption, 73, 641, 25, C.sub, 1130, 34));
  };
}
