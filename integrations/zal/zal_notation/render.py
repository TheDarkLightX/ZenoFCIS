"""Deterministic SVG projections of grammar-bound ZAL models.

The renderer is deliberately inert: packs contain geometry and escaped labels,
not code. Embedded source makes the model recoverable; it does not certify SVG
geometry subsequently edited by another program. ``tromp`` is a dedicated
closed-lambda projection using John Tromp's published binding conventions.
"""
from __future__ import annotations

import math
import textwrap
from html import escape
from itertools import pairwise
from typing import Any

from . import core

RENDERER = "zal.svg/1"
MAX_DIMENSION = 12000
MAX_SVG_BYTES = 2_000_000
CELL_W = 140.0
CELL_H = 100.0
STEP_X = 200.0
STEP_Y = 190.0
MARGIN = 70.0
TOP = 86.0
COLORS = {
    "ink": "#1f2937",
    "accent": "#4f46a5",
    "muted": "#626b77",
    "paper": "#ffffff",
    "none": "none",
}


def _fail(message: str) -> None:
    raise core.NotationError(message)


def _n(value: float) -> str:
    if not math.isfinite(value):
        _fail("Nonfinite rendering coordinate")
    return str(int(value)) if value == int(value) else f"{value:.3f}".rstrip("0").rstrip(".")


def _attrs(**attrs: Any) -> str:
    return " ".join(f'{key.replace("_", "-")}="{escape(str(value), quote=True)}"' for key, value in attrs.items())


def _source(ref: str, role: str | None = None, **extra: Any) -> str:
    attrs = {"data-model-ref": ref, **extra}
    if role:
        attrs["data-role"] = role
    return _attrs(**attrs)


def _line(x1: float, y1: float, x2: float, y2: float, ref: str, role: str = "wire", **extra: Any) -> str:
    return f'<line x1="{_n(x1)}" y1="{_n(y1)}" x2="{_n(x2)}" y2="{_n(y2)}" {_source(ref, role, **extra)} stroke="{COLORS["ink"]}" stroke-width="1.6"/>'


def _short(value: str, limit: int) -> str:
    return value if len(value) <= limit else value[:max(0, limit - 1)] + "…"


def _text(x: float, y: float, value: str, ref: str, *, anchor: str = "middle", color: str = "ink", size: int = 13, limit: int = 80, role: str = "label") -> str:
    shown = _short(value, limit)
    title = f"<title>{escape(value)}</title>" if shown != value else ""
    return f'<text x="{_n(x)}" y="{_n(y)}" text-anchor="{anchor}" fill="{COLORS[color]}" font-size="{size}" {_source(ref, role)}>{title}{escape(shown)}</text>'


def _caption(x: float, y: float, value: str, ref: str, limit: int = 23, *, background: bool = False) -> str:
    # Full captions are always retained in aria-label and model metadata.
    lines = textwrap.wrap(value, width=limit, break_long_words=True, break_on_hyphens=False) or [""]
    if len(lines) > 2:
        lines = [lines[0], _short(" ".join(lines[1:]), limit)]
    out = f'<g {_source(ref, "caption")} aria-label="{escape(value, quote=True)}">'
    if background and value:
        width = max(len(line) for line in lines) * 7 + 10
        out += f'<rect x="{_n(x-width/2)}" y="{_n(y-13)}" width="{_n(width)}" height="{len(lines)*16+5}" fill="{COLORS["paper"]}" {_source(ref,"caption-background")}/>'
    for index, line in enumerate(lines):
        out += _text(x, y + index * 16, line, ref, size=13, limit=limit)
    return out + "</g>"


def _path(d: str, ref: str, *, role: str = "path", stroke: str = "ink", fill: str = "none", extra: str = "") -> str:
    return f'<path d="{d}" {_source(ref, role)} stroke="{COLORS[stroke]}" fill="{COLORS[fill]}" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round" {extra}/>'


def _glyph(glyph: dict, fields: dict, tag: str, node_id: str, x: float, y: float, ref: str) -> str:
    def px(value: float) -> float:
        return x + value * CELL_W / 100

    def py(value: float) -> float:
        return y + value * CELL_H / 100

    out = f'<g {_source(ref, "glyph")}>'
    for index, item in enumerate(glyph["primitives"]):
        kind = item["kind"]
        source = _source(ref, "primitive", **{"data-primitive-index": index, "data-primitive-kind": kind})
        stroke = COLORS[item.get("stroke", "ink")]
        fill = COLORS[item.get("fill", "none")]
        common = f'{source} stroke="{stroke}" fill="{fill}" stroke-width="1.6"'
        if kind == "line":
            out += f'<line x1="{_n(px(item["x1"]))}" y1="{_n(py(item["y1"]))}" x2="{_n(px(item["x2"]))}" y2="{_n(py(item["y2"]))}" {common}/>'
        elif kind == "rect":
            if item["x"] + item["width"] > 100 or item["y"] + item["height"] > 100:
                _fail("Rectangle exceeds its normalized glyph cell")
            out += f'<rect x="{_n(px(item["x"]))}" y="{_n(py(item["y"]))}" width="{_n(item["width"] * CELL_W / 100)}" height="{_n(item["height"] * CELL_H / 100)}" {common}/>'
        elif kind == "ellipse":
            if item["cx"] - item["rx"] < 0 or item["cx"] + item["rx"] > 100 or item["cy"] - item["ry"] < 0 or item["cy"] + item["ry"] > 100:
                _fail("Ellipse exceeds its normalized glyph cell")
            out += f'<ellipse cx="{_n(px(item["cx"]))}" cy="{_n(py(item["cy"]))}" rx="{_n(item["rx"] * CELL_W / 100)}" ry="{_n(item["ry"] * CELL_H / 100)}" {common}/>'
        elif kind == "polygon":
            points = " ".join(f"{_n(px(p[0]))},{_n(py(p[1]))}" for p in item["points"])
            out += f'<polygon points="{points}" {common}/>'
        elif kind == "path":
            commands = []
            for command in item["commands"]:
                if command[0] == "Z":
                    commands.append("Z")
                else:
                    coordinates = [px(value) if k % 2 == 0 else py(value) for k, value in enumerate(command[1:])]
                    commands.append(command[0] + " " + " ".join(_n(value) for value in coordinates))
            out += f'<path d="{" ".join(commands)}" {common} stroke-linejoin="round"/>'
        elif kind == "text":
            value = core.format_label(item["text"], fields, tag=tag, node_id=node_id)
            # Text marks are single-line; truncation is visible and the complete
            # value is retained as a native SVG title and source metadata.
            room = min(item["x"], 100 - item["x"]) * CELL_W / 100 * 2
            out += _text(px(item["x"]), py(item["y"]), value, ref, size=12, color=item.get("fill", "ink"), limit=max(1, int(room / 7)), role="primitive-text")
        else:  # Defensive even when the core validator has already rejected it.
            _fail(f"Unsupported glyph primitive: {kind}")
    label = core.format_label(glyph.get("label", "{_tag}"), fields, tag=tag, node_id=node_id)
    out += _caption(x + CELL_W / 2, y + CELL_H + 23, label, ref, background=True)
    return out + "</g>"


def _all_terms(term: dict, path: str = "root") -> list[tuple[str, dict]]:
    result = [(path, term)]
    for index, child in enumerate(term["children"]):
        result.extend(_all_terms(child, f"{path}.children.{index}"))
    return result


def _tree(model: dict, pack: dict, folded: tuple[str, ...]) -> tuple[str, float, float]:
    body = model["body"]
    terms = dict(_all_terms(body))
    if any(path not in terms for path in folded):
        _fail("A folded path does not identify a term")
    folds = set(folded)
    widths: dict[str, int] = {}
    positions: dict[str, tuple[float, float]] = {}

    def measure(term: dict, path: str) -> int:
        child_widths = [] if path in folds else [measure(child, f"{path}.children.{i}") for i, child in enumerate(term["children"])]
        widths[path] = max(1, sum(child_widths))
        return widths[path]

    measure(body, "root")
    width = max(360.0, MARGIN * 2 + widths["root"] * STEP_X)
    if width > MAX_DIMENSION:
        _fail("Tree rendering exceeds the width budget; fold subterms")

    def place(term: dict, path: str, left: float, depth: int) -> None:
        positions[path] = (left + widths[path] * STEP_X / 2 - CELL_W / 2, TOP + depth * STEP_Y)
        if path in folds:
            return
        for index, child in enumerate(term["children"]):
            cp = f"{path}.children.{index}"
            place(child, cp, left, depth + 1)
            left += widths[cp] * STEP_X

    place(body, "root", (width - widths["root"] * STEP_X) / 2, 0)
    height = max(y for _, y in positions.values()) + CELL_H + 95
    if height > MAX_DIMENSION:
        _fail("Tree rendering exceeds the height budget; fold subterms")
    out = []
    # Edges precede glyphs, so a crossing never becomes a data-copy junction.
    for path, (x, y) in positions.items():
        if path in folds:
            continue
        for index, _ in enumerate(terms[path]["children"]):
            cp = f"{path}.children.{index}"
            cx, cy = positions[cp]
            d = f"M {_n(x+CELL_W/2)} {_n(y+CELL_H+43)} C {_n(x+CELL_W/2)} {_n(y+CELL_H+65)}, {_n(cx+CELL_W/2)} {_n(cy-22)}, {_n(cx+CELL_W/2)} {_n(cy)}"
            out.append(_path(d, cp, role="ordered-child"))
            out.append(_text(cx + CELL_W / 2 + 12, cy - 8, str(index), cp, anchor="start", color="muted", size=11))
    for path, (x, y) in positions.items():
        term = terms[path]
        glyph = pack["glyphs"][term["tag"]]
        if path in folds and term["children"]:
            hidden = len(_all_terms(term)) - 1
            d = f"M {_n(x+15)} {_n(y)} H {_n(x)} V {_n(y+CELL_H)} H {_n(x+15)} M {_n(x+CELL_W-15)} {_n(y)} H {_n(x+CELL_W)} V {_n(y+CELL_H)} H {_n(x+CELL_W-15)}"
            out.append(_path(d, path, role="fold-contour"))
            out.append(_text(x + CELL_W - 16, y + 20, "≔", path, role="definition-mark"))
            out.append(_text(x + CELL_W / 2, y + 47, term["tag"], path, limit=18))
            out.append(_text(x + CELL_W / 2, y + 72, f"{hidden} retained nodes", path, size=12, color="muted", role="fold-summary"))
            label = core.format_label(glyph.get("label", "{_tag}"), term["fields"], tag=term["tag"], node_id=path)
            out.append(_caption(x + CELL_W / 2, y + CELL_H + 23, label, path, background=True))
        else:
            out.append(_glyph(glyph, term["fields"], term["tag"], path, x, y, path))
    return "".join(out), width, height


def _wave(points: list[tuple[float, float]]) -> list[tuple[float, float]]:
    distances = [0.0]
    for a, b in pairwise(points):
        distances.append(distances[-1] + math.dist(a, b))
    length = distances[-1]
    periods = max(2, round(length / 28))
    samples = min(4096, max(48, periods * 12))
    result = []
    segment = 0
    for i in range(samples + 1):
        distance = length * i / samples
        while segment < len(points) - 2 and distances[segment + 1] < distance:
            segment += 1
        a, b = points[segment], points[segment + 1]
        norm = max(1e-9, distances[segment + 1] - distances[segment])
        fraction = (distance - distances[segment]) / norm
        x, y = a[0] + fraction * (b[0] - a[0]), a[1] + fraction * (b[1] - a[1])
        dx, dy = b[0] - a[0], b[1] - a[1]
        offset = 4.5 * math.sin(2 * math.pi * periods * i / samples)
        result.append((x - dy / norm * offset, y + dx / norm * offset))
    return result

def _graph(grammar: dict, model: dict, pack: dict, marker_id: str, *, compact_labels: bool = False) -> tuple[str, float, float]:
    body = model["body"]
    nodes = body["nodes"]
    count = len(nodes)
    cols = max(1, math.ceil(math.sqrt(count)))
    loop_counts: dict[str, int] = {}
    for edge in body["edges"]:
        if edge["source"][0] == edge["target"][0]:
            node_id = edge["source"][0]
            loop_counts[node_id] = loop_counts.get(node_id, 0) + 1
    max_loops = max(loop_counts.values(), default=0)
    if max_loops > 9:
        _fail("Too many self-loops for the bounded graph layout")
    margin = MARGIN + max_loops * 18
    top = TOP + (45 + max_loops * 28 if max_loops else 0)
    step_x, step_y = 380.0, 280.0 + max_loops * 28
    positions = {node["id"]: (margin + (i % cols)*step_x, top + (i // cols)*step_y) for i, node in enumerate(nodes)}
    by_id = {node["id"]: node for node in nodes}
    width = max(440.0, margin*2 + (cols-1)*step_x + CELL_W + 95)
    rows = max(1, math.ceil(count/cols))
    height = top + (rows-1)*step_y + CELL_H + 120
    if max(width, height) > MAX_DIMENSION:
        _fail("Graph layout exceeds its dimension budget")
    out = []
    pair_counts: dict[tuple[str, str], int] = {}
    edge_labels = [core.format_label(pack["edges"][edge["tag"]]["label"], edge["fields"], tag=edge["tag"], node_id=edge["id"]) for edge in body["edges"]]
    legend_mode = (compact_labels or len(body["edges"]) > 8
                   or any(len(textwrap.wrap(label, width=25, break_long_words=True,
                                            break_on_hyphens=False)) > 2 for label in edge_labels))
    # Keep labels away from nodes and from each other. This is presentation
    # layout only; the typed source identity is independent of these choices.
    occupied = [(x-12, y-10, CELL_W+24, CELL_H+61) for x, y in positions.values()]

    def collides(box: tuple[float, float, float, float]) -> bool:
        x, y, w, h = box
        if x < 12 or x+w > width-12 or y < 67 or y+h > height-12:
            return True
        return any(x < ox+ow+5 and x+w+5 > ox and y < oy+oh+5 and y+h+5 > oy for ox, oy, ow, oh in occupied)

    def label_at(value: str, ref: str, points: list[tuple[float, float]], preferred: tuple[float, float] | None = None) -> None:
        if not value:
            return
        lines = textwrap.wrap(value, width=25, break_long_words=True, break_on_hyphens=False) or [""]
        if len(lines) > 2:
            lines = [lines[0], _short(" ".join(lines[1:]), 25)]
        box_w = min(188.0, max(len(line) for line in lines)*7.0+12)
        box_h = 16*len(lines)+8
        candidates = [preferred] if preferred else []
        for index in (24, 16, 32, 8, 40):
            x, y = points[index]
            before, after = points[max(0,index-1)], points[min(len(points)-1,index+1)]
            dx, dy = after[0]-before[0], after[1]-before[1]
            norm = max(1.0, math.hypot(dx,dy))
            for distance in (24, -24, 45, -45, 70, -70, 95, -95):
                candidates.append((x-dy/norm*distance, y+dx/norm*distance))
        chosen = None
        for candidate in candidates:
            if candidate is None:
                continue
            x,y = candidate
            box = (x-box_w/2, y-13, box_w, box_h)
            if not collides(box):
                chosen = (x,y,box)
                break
        if chosen is None:
            # A cluttered graph should explicitly refuse this automatic layout
            # instead of exporting unreadable or silently omitted labels.
            _fail("Edge labels exceed the automatic layout budget; simplify the view or captions")
        x,y,box = chosen
        occupied.append(box)
        out.append(f'<rect x="{_n(box[0])}" y="{_n(box[1])}" width="{_n(box[2])}" height="{_n(box[3])}" fill="{COLORS["paper"]}" {_source(ref,"label-background")}/>')
        out.append(_caption(x,y,value,ref,limit=25))

    def normal(anchor: list) -> tuple[float,float]:
        x,y = anchor
        distances = ((x,(-1,0)),(100-x,(1,0)),(y,(0,-1)),(100-y,(0,1)))
        return min(distances,key=lambda p:p[0])[1]

    for edge_index, edge in enumerate(body["edges"]):
        ref = f'edges.{edge["id"]}'
        sid, sport = edge["source"]
        tid, tport = edge["target"]
        source, target = by_id[sid], by_id[tid]
        sx, sy = positions[sid]
        tx, ty = positions[tid]
        sa = pack["glyphs"][source["tag"]]["anchors"][sport]
        ta = pack["glyphs"][target["tag"]]["anchors"][tport]
        start = (sx+sa[0]*CELL_W/100, sy+sa[1]*CELL_H/100)
        end = (tx+ta[0]*CELL_W/100, ty+ta[1]*CELL_H/100)
        rule = pack["edges"][edge["tag"]]
        style = rule["style"]
        pair = tuple(sorted((sid, tid)))
        lane = pair_counts.get(pair, 0)
        pair_counts[pair] = lane+1
        if lane > 8:
            _fail("Too many parallel edges for the bounded graph layout")
        preferred = None
        if sid == tid:
            # Route outside the complete node, not through its drawing. The
            # left/right anchors can therefore return to the same state.
            pad = 40 + lane*18
            high = sy-50-lane*28
            route = [start,(sx+CELL_W+pad,start[1]),(sx+CELL_W+pad,high),
                     (sx-pad,high),(sx-pad,end[1]),end]
            distances = [0.0]
            for a,b in pairwise(route):
                distances.append(distances[-1]+math.dist(a,b))
            points=[]
            for i in range(49):
                at=distances[-1]*i/48
                j=next((j for j in range(len(route)-1) if at <= distances[j+1]),len(route)-2)
                fraction=(at-distances[j])/max(1e-9,distances[j+1]-distances[j])
                a,b=route[j],route[j+1]
                points.append((a[0]+fraction*(b[0]-a[0]),a[1]+fraction*(b[1]-a[1])))
            preferred=(sx+CELL_W/2,high-15)
        else:
            sn,tn=normal(sa),normal(ta)
            distance=max(1.0,math.dist(start,end))
            reach=min(135.0,max(60.0,distance*.3))+lane*12
            c1=(min(width-20,max(20,start[0]+sn[0]*reach)),min(height-25,max(75,start[1]+sn[1]*reach)))
            c2=(min(width-20,max(20,end[0]+tn[0]*reach)),min(height-25,max(75,end[1]+tn[1]*reach)))
            # Parallel edges get distinct route lanes while preserving ports.
            if lane:
                c1=(c1[0],max(75,c1[1]-lane*30))
                c2=(c2[0],max(75,c2[1]-lane*30))
            points=[((1-t)**3*start[0]+3*(1-t)**2*t*c1[0]+3*(1-t)*t*t*c2[0]+t**3*end[0],
                     (1-t)**3*start[1]+3*(1-t)**2*t*c1[1]+3*(1-t)*t*t*c2[1]+t**3*end[1])
                    for t in (i/48 for i in range(49))]
        base_points=points
        if style == "wavy":
            points=_wave(points)
        d="M "+" L ".join(f"{_n(x)} {_n(y)}" for x,y in points)
        extra=f'data-edge-style="{style}"'
        if style == "dashed":
            extra+=' stroke-dasharray="6 5"'
        if rule["arrow"]:
            extra+=f' marker-end="url(#{marker_id})"'
        out.append(_path(d,ref,role="edge",extra=extra))
        label = str(edge_index + 1) if legend_mode else edge_labels[edge_index]
        try:
            label_at(label, ref, base_points, preferred)
        except core.NotationError:
            if not legend_mode:
                return _graph(grammar, model, pack, marker_id, compact_labels=True)
            raise
    for node in nodes:
        ref=f'nodes.{node["id"]}'
        x,y=positions[node["id"]]
        glyph=pack["glyphs"][node["tag"]]
        out.append(_glyph(glyph,node["fields"],node["tag"],node["id"],x,y,ref))
        for name,anchor in glyph["anchors"].items():
            px,py=x+anchor[0]*CELL_W/100,y+anchor[1]*CELL_H/100
            out.append(f'<circle cx="{_n(px)}" cy="{_n(py)}" r="3.5" {_source(ref,"port",**{"data-port":name})} fill="{COLORS["paper"]}" stroke="{COLORS["ink"]}" stroke-width="1.3"/>')
    if legend_mode:
        y = height - 18
        out.append(_line(20, y, width - 20, y, "model", "legend-separator"))
        y += 28
        out.append(_text(20, y, "Edges — complete labels", "model", anchor="start", size=14, role="legend-title"))
        y += 28
        columns = min(120, max(25, int((width - 42) / 7)))
        for index, (edge, label) in enumerate(zip(body["edges"], edge_labels)):
            ref = f'edges.{edge["id"]}'
            heading = f'{index+1} · {edge["id"]} · {edge["tag"]}'
            lines = textwrap.wrap(heading, width=columns, replace_whitespace=False, drop_whitespace=False) or [""]
            detail_lines = textwrap.wrap(label, width=columns, replace_whitespace=False, drop_whitespace=False) or ["(unlabelled)"]
            out.append(f'<g {_source(ref,"edge-legend")} aria-label="{escape(heading + ": " + label, quote=True)}">')
            for line in lines:
                out.append(_text(20, y, line, ref, anchor="start", size=13, limit=len(line), role="legend-edge-id"))
                y += 17
            out.append(f'<g {_source(ref,"caption")} aria-label="{escape(label, quote=True)}">')
            for line in detail_lines:
                out.append(_text(20, y, line, ref, anchor="start", size=12, limit=len(line), color="muted", role="legend-edge-label"))
                y += 17
            out.append("</g></g>")
            y += 13
            if y > MAX_DIMENSION - 20:
                _fail("Complete edge legend exceeds its height budget")
        height = y + 12
    return "".join(out),width,height

def _tromp(model: dict) -> tuple[str, float, float]:
    body = model["body"]
    leaves: dict[str, int] = {}

    def count(term: dict, path: str) -> int:
        tag = term["tag"]
        if tag == "Var":
            value = 1
        elif tag == "Lam":
            value = count(term["children"][0], path+".children.0")
        elif tag == "App":
            value = sum(count(child, path+f".children.{i}") for i, child in enumerate(term["children"]))
        else:
            _fail("Tromp projection requires the declared Var/Lam/App profile")
        leaves[path] = value
        return value

    leaf_count = count(body, "root")
    gap_x, gap_y = 42.0, 28.0
    width = max(360.0, leaf_count*gap_x + 2*MARGIN)
    if width > MAX_DIMENSION:
        _fail("Tromp rendering exceeds its occurrence-width budget")
    offset = (width-(leaf_count-1)*gap_x)/2
    out: list[str] = []

    def visit(term: dict, path: str, col: int, y: float, scope: tuple) -> tuple[float, float, str]:
        tag = term["tag"]
        if tag == "Var":
            name = term["fields"]["name"]
            binding = next((binder for binder in reversed(scope) if binder[0] == name), None)
            if binding is None:
                _fail(f"Tromp closed-term profile: unbound variable {name!r}")
            _, binder_ref, binder_y = binding
            x = offset + col*gap_x
            out.append(_line(x, binder_y, x, y, path, "occurrence", **{"data-binder-ref":binder_ref}))
            return x, y, path
        if tag == "Lam":
            name = term["fields"]["name"]
            left = offset + col*gap_x-15
            right = offset + (col+leaves[path]-1)*gap_x+15
            out.append(_line(left, y, right, y, path, "binder", **{"data-binder-name":name}))
            return visit(term["children"][0], path+".children.0", col, y+gap_y, scope+((name,path,y),))
        left_path, right_path = path+".children.0", path+".children.1"
        lx, ly, lr = visit(term["children"][0], left_path, col, y, scope)
        rx, ry, rr = visit(term["children"][1], right_path, col+leaves[left_path], y, scope)
        join_y = max(ly, ry)+gap_y
        if ly < join_y:
            out.append(_line(lx, ly, lx, join_y, lr, "occurrence-extension"))
        if ry < join_y:
            out.append(_line(rx, ry, rx, join_y, rr, "occurrence-extension"))
        out.append(_line(lx, join_y, rx, join_y, path, "application"))
        return lx, join_y, lr

    _, bottom, _ = visit(body, "root", 0, TOP, ())
    height = max(190.0, bottom+85)
    if height > MAX_DIMENSION:
        _fail("Tromp rendering exceeds its depth-height budget")
    out.append(_text(width/2, height-39, "Binders · occurrences · applications", "model", size=12, color="muted", limit=max(18,int(width/7))))
    return "".join(out), width, height


def render_svg(grammar: dict, model: dict, pack: dict, *, folded: tuple[str, ...] = ()) -> str:
    """Render a complete, validated model as an inert, source-bound SVG.

    ``folded`` contains exact term paths such as ``root.children.0``. The entire
    model is validated before folding. Graph folding and Tromp subtree folding
    are refused rather than assigned an implicit abstraction interpretation.
    """
    core.validate_model(grammar, model)
    core.validate_pack(grammar, pack)
    if not isinstance(folded, (tuple, list)) or any(not isinstance(path, str) for path in folded):
        _fail("folded must be a finite sequence of term paths")
    if len(folded) > 512:
        _fail("Too many folded paths")
    folded = tuple(sorted(set(folded)))
    model_id = core.model_digest(grammar, model)
    notation_id = core.digest(pack)
    view_id = core.digest({"folded":list(folded),"renderer":RENDERER})
    prefix = "zal-" + core.digest([model_id, notation_id, view_id])[:18]
    marker_id = prefix+"-arrow"
    if pack["layout"] == "tromp":
        if model["kind"] != "term" or folded:
            _fail("Tromp projection needs a closed term and no collapsed subterms")
        diagram, width, height = _tromp(model)
    elif model["kind"] == "term" and pack["layout"] == "tree":
        diagram, width, height = _tree(model, pack, folded)
    elif model["kind"] == "graph" and pack["layout"] == "graph":
        if folded:
            _fail("Graph folding is not defined by this rendering profile")
        diagram, width, height = _graph(grammar, model, pack, marker_id)
    else:
        _fail("Notation layout and model kind are incompatible")
    if max(width, height) > MAX_DIMENSION:
        _fail("Rendering exceeds the maximum SVG dimensions")
    title = f'ZAL notation · {pack["id"]}'
    description = "Structural rendering of a grammar-bound model. Coordinates and styles do not grant semantic verification or execution authority. Full source is embedded as metadata."
    payload = {
        "schema":"zal.diagram-source/1", "grammar":grammar, "model":model,
        "notation":pack, "view":{"folded":list(folded)},
        "model_digest":model_id, "notation_digest":notation_id,
        "renderer":RENDERER,
    }
    # Apply the same aggregate budget as recovery, so successful output always
    # carries a source envelope accepted by the source reader.
    metadata = escape(core.canonical(payload))
    svg = f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {_n(width)} {_n(height)}" width="{_n(width)}" height="{_n(height)}" role="img" aria-labelledby="{prefix}-title {prefix}-description" font-family="system-ui, sans-serif" data-model-digest="{model_id}" data-notation-digest="{notation_id}" data-view-digest="{view_id}">'
    svg += f'<title id="{prefix}-title">{escape(title)}</title><desc id="{prefix}-description">{escape(description)}</desc>'
    svg += f'<metadata id="zal-source">{metadata}</metadata>'
    svg += f'<defs><marker id="{marker_id}" viewBox="0 0 10 10" refX="9" refY="5" markerWidth="7" markerHeight="7" orient="auto-start-reverse"><path d="M 0 0 L 10 5 L 0 10 Z" fill="{COLORS["ink"]}"/></marker></defs>'
    svg += f'<rect width="100%" height="100%" fill="{COLORS["paper"]}" data-model-ref="renderer"/>'
    svg += _text(20, 29, title, "model", anchor="start", size=15, limit=max(20,int((width-40)/8)))
    svg += _text(20, 50, "Structural view · no semantic check asserted", "model", anchor="start", color="muted", size=12, limit=max(20,int((width-40)/7)))
    svg += diagram + "</svg>"
    if len(svg.encode("utf-8")) > MAX_SVG_BYTES:
        _fail("SVG output exceeds its byte budget")
    return svg
