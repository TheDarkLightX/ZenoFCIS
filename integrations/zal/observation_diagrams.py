#!/usr/bin/env python3
"""Bounded, deterministic diagrams of exact observation arenas and checked beliefs.

The diagrams are views, not a new proof procedure. Every drawn edge has an exact
row in the accompanying JSON. Belief views require an independently checked
arena certificate; a caller cannot color a belief winning by supplying a label.
"""
from __future__ import annotations

import argparse
from collections import deque
import hashlib
import json
from pathlib import Path
import sys
import textwrap
from xml.sax.saxutils import escape

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[1]
sys.path.insert(0, str(ROOT / "tools" / "trace_game"))
from arena_check import check as check_certificate
from arena_spec import parse as parse_arena

MAX_NODES = 64
MAX_EDGES = 256
MAX_LABEL = 512
SCHEMA = "zal/observation-diagrams/1"


def _canonical(value):
    return json.dumps(value, sort_keys=True, separators=(",", ":"), ensure_ascii=True)


def _digest(value):
    return hashlib.sha256(_canonical(value).encode()).hexdigest()


def _need(condition, message):
    if not condition:
        raise ValueError(message)


def _label(value):
    _need(type(value) is str and len(value) <= MAX_LABEL, "diagram label bound")
    _need(all(ord(c) >= 32 or c in "\t\n\r" for c in value), "diagram XML character")
    _need(all(not 0xD800 <= ord(c) <= 0xDFFF for c in value), "diagram XML character")
    return value


def _check_size(nodes, edges):
    _need(0 < len(nodes) <= MAX_NODES, "diagram node bound (64)")
    _need(sum(len(row) for row in edges) <= MAX_EDGES, "diagram edge bound (256)")


def _text(x, y, value, cls="body"):
    return f'<text x="{x}" y="{y}" class="{cls}">{escape(value)}</text>'


def _lines(value, width=29, maximum=3):
    lines = textwrap.wrap(value, width, break_long_words=True, break_on_hyphens=False) or [""]
    if len(lines) > maximum:
        lines = lines[:maximum]
        lines[-1] = lines[-1][:-1] + "…"
    return lines


def _observation_label(value):
    """Shorten only the adapter's exact registered observation shapes."""
    try:
        parsed = json.loads(value)
    except ValueError:
        return value
    if type(parsed) is dict and set(parsed) == {"phase", "view"} and parsed["phase"] == "world":
        return str(parsed["view"])
    if (type(parsed) is dict and set(parsed) == {"phase", "action", "outcome", "reason"}
            and parsed["phase"] == "response"):
        suffix = "" if parsed["reason"] is None else "/" + str(parsed["reason"])
        return str(parsed["action"]) + " → " + str(parsed["outcome"]) + suffix
    return value


def _view(title, nodes, owners, observations, edges, roots, bad, winning=None, node_prefix="s"):
    """Render all vertices/edges; short edge IDs link the graph to its full legend."""
    _check_size(nodes, edges)
    distance = {i: 0 for i in roots}
    queue = deque(roots)
    while queue:
        source = queue.popleft()
        for _, target in edges[source]:
            if target not in distance:
                distance[target] = distance[source] + 1
                queue.append(target)
    end = max(distance.values(), default=-1) + 1
    for i in range(len(nodes)):
        distance.setdefault(i, end)
    rows = []
    for level in sorted(set(distance.values())):
        ids = [i for i in range(len(nodes)) if distance[i] == level]
        rows.extend(ids[j:j + 4] for j in range(0, len(ids), 4))
    width, card_w, card_h = 1160, 240, 108
    positions = {}
    for row, ids in enumerate(rows):
        offset = (width - (len(ids) * card_w + (len(ids) - 1) * 40)) // 2
        for col, i in enumerate(ids):
            positions[i] = (offset + col * (card_w + 40), 138 + row * 208)
    graph_bottom = max(y for _, y in positions.values()) + card_h + 85
    flat = [dict(id=f"e{j + 1}", source=i, action=a, target=t)
            for j, (i, a, t) in enumerate((i, a, t) for i, row in enumerate(edges) for a, t in row)]
    legend_rows = []
    for edge in flat:
        action_lines = _lines(edge["action"], width=92, maximum=8)
        legend_rows.append((edge, action_lines, max(36, 20 + 18 * len(action_lines))))
    height = graph_bottom + 100 + sum(h for _, _, h in legend_rows)
    out = [f'<svg xmlns="http://www.w3.org/2000/svg" width="{width}" height="{height}" viewBox="0 0 {width} {height}" role="img" aria-labelledby="title description">',
           f'<title id="title">{escape(title)}</title>',
           '<desc id="description">Every node and edge of the declared finite graph is shown. Edge IDs identify exact transitions in the legend and JSON companion. Blue denotes controller choice; gray denotes environment choice. Red marks a bad state. Root and winning annotations are also written as text.</desc>',
           '<style>text{font-family:ui-sans-serif,system-ui,sans-serif;fill:#172438}.title{font-size:27px;font-weight:700}.body{font-size:14px}.small{font-size:12px;fill:#41546a}.name{font-size:15px;font-weight:650}.edgeid{font-size:11px;font-weight:700;fill:#273f59}.legend{font-size:14px}.node{stroke-width:2}.edge{fill:none;stroke:#70849b;stroke-width:1.5}</style>',
           '<defs><marker id="arrow" markerWidth="7" markerHeight="7" refX="6" refY="3.5" orient="auto"><path d="M0,0 L7,3.5 L0,7 z" fill="#70849b"/></marker></defs>',
           f'<rect width="{width}" height="{height}" fill="#f8fafc"/>',
           _text(36, 46, title, "title"),
           _text(36, 75, "Exact finite graph • blue: controller • gray: environment • red: bad • roots are marked", "body"),
           _text(36, 98, "Edge IDs refer to the complete legend. Full labels and observations are retained in the JSON companion.", "small")]
    for edge in flat:
        i, t = edge["source"], edge["target"]
        x, y = positions[i]; tx, ty = positions[t]
        sx, sy, ex, ey = x + card_w // 2, y + card_h, tx + card_w // 2, ty
        ordinal = int(edge["id"][1:])
        if i == t:
            side = -1 if x + card_w // 2 < width // 2 else 1
            sx, sy, ex, ey = x if side < 0 else x + card_w, y + 25, x if side < 0 else x + card_w, y + 78
            reach = side * 30
            path = f"M {sx},{sy} C {sx + reach},{sy - 24} {ex + reach},{ey + 24} {ex},{ey}"
            lx, ly = sx - 29 if side < 0 else sx + 7, (sy + ey) // 2
        elif ty > y:
            middle = (sy + ey) // 2
            path = f"M {sx},{sy} C {sx},{middle} {ex},{middle} {ex},{ey}"
            fraction = 0.30
            spread = 3 * fraction ** 2 - 2 * fraction ** 3
            lx = round(sx + spread * (ex - sx))
            ly = round((1 - fraction) ** 3 * sy + 3 * (1 - fraction) ** 2 * fraction * middle
                       + 3 * (1 - fraction) * fraction ** 2 * middle + fraction ** 3 * ey) - 3
        else:
            sx, sy, ex, ey = x, y + 65, tx, ty + 42
            lane = max(14, min(x, tx) - 18 - ordinal % 5 * 6)
            path = f"M {sx},{sy} C {lane},{sy} {lane},{ey} {ex},{ey}"
            lx, ly = lane - 8, (sy + ey) // 2
        out.append(f'<path class="edge" d="{path}" marker-end="url(#arrow)" data-edge="{edge["id"]}" data-source="{i}" data-target="{t}"><title>{escape(edge["action"])}</title></path>')
        out.append(f'<rect x="{lx - 5}" y="{ly - 11}" width="32" height="16" rx="4" fill="#f8fafc"/>')
        out.append(_text(lx, ly + 1, edge["id"], "edgeid"))
    for i, name in enumerate(nodes):
        x, y = positions[i]
        owner = "controller" if owners[i] == 0 else "environment"
        fill, stroke = ("#eaf2ff", "#4377b8") if owners[i] == 0 else ("#eef1f5", "#718196")
        if i in bad:
            fill, stroke = "#ffebec", "#bd4553"
        tags = [f"{node_prefix}{i}", owner]
        if i in roots: tags.append("root")
        if i in bad: tags.append("BAD")
        if winning is not None: tags.append("winning" if i in winning else "losing")
        out.append(f'<g data-node="{i}"><title>{escape(name)}; observation: {escape(observations[i])}</title><rect class="node" x="{x}" y="{y}" width="{card_w}" height="{card_h}" rx="11" fill="{fill}" stroke="{stroke}"/>')
        out.append(_text(x + 12, y + 21, " · ".join(tags), "small"))
        for j, line in enumerate(_lines(name, maximum=2)):
            out.append(_text(x + 12, y + 43 + j * 18, line, "name"))
        observation = _lines("observe: " + _observation_label(observations[i]), maximum=1)[0]
        out.append(_text(x + 12, y + 91, observation, "small"))
        out.append('</g>')
    out.extend([_text(36, graph_bottom, "Complete transition legend", "title"),
                _text(36, graph_bottom + 27, "These rows define every drawn arrow; no state or edge is omitted.", "body")])
    y = graph_bottom + 58
    for edge, lines, step in legend_rows:
        out.append(_text(36, y, edge["id"], "name"))
        out.append(_text(92, y, f'{node_prefix}{edge["source"]} → {node_prefix}{edge["target"]}', "name"))
        for j, line in enumerate(lines):
            out.append(_text(240, y + j * 18, line, "legend"))
        y += step
    out.append('</svg>')
    return "\n".join(out) + "\n", flat


def render(arena_wire, labels=None, certificate=None):
    """Create a checked graph view; labels are annotations, never semantic input."""
    arena = parse_arena(arena_wire)
    _need(arena.kind == "safety", "diagram profile supports safety arenas only")
    _check_size(arena.owners, arena.edges)
    for value in arena.observations:
        _label(value)
    for row in arena.edges:
        for action, _ in row:
            _label(action)
    if labels is None:
        labels = {i: f"state {i}" for i in range(len(arena.owners))}
    _need(type(labels) is dict and all(type(i) is int for i in labels)
          and set(labels) == set(range(len(arena.owners))), "diagram label coverage")
    names = [_label(labels[i]) for i in range(len(arena.owners))]
    wire = arena.wire()
    metadata = dict(schema=SCHEMA, arena_digest=_digest(wire), arena=wire,
                    labels={str(i): names[i] for i in range(len(names))},
                    certificate_digest=None, verdict=None, knowledge=None,
                    scope="exact finite-model presentation; no native execution or runtime authority")
    title = "ZAL observation game · " + ("partial observation" if arena.partial else "perfect observation")
    arena_svg, metadata["arena_edges"] = _view(title, names, arena.owners, arena.observations,
                                              arena.edges, arena.roots, arena.marks)
    knowledge_svg = None
    if certificate is not None:
        if arena.partial:
            _check_size(certificate["knowledge"]["nodes"], certificate["knowledge"]["edges"])
        verdict = check_certificate(certificate, arena)
        metadata.update(certificate_digest=_digest(certificate), verdict=verdict)
        if arena.partial:
            graph = certificate["knowledge"]
            _check_size(graph["nodes"], graph["edges"])
            knowledge_names = []
            observations = []
            for node in graph["nodes"]:
                members = ", ".join(f"s{i}" for i in node["belief"])
                suffix = "" if node["action"] is None else " / " + node["action"]
                knowledge_names.append("{" + members + "}" + suffix)
                observations.append(arena.observations[node["belief"][0]])
            knowledge_svg, knowledge_edges = _view(
                "Checked beliefs · " + verdict["evidence"], knowledge_names,
                graph["owners"], observations, graph["edges"], graph["roots"],
                [i for i, bad in enumerate(graph["bad"]) if bad],
                set(certificate["certificate"]["winning"]), node_prefix="b")
            metadata["knowledge"] = dict(graph=json.loads(_canonical(graph)), edges=knowledge_edges)
    return dict(arena_svg=arena_svg, knowledge_svg=knowledge_svg, metadata=metadata)


def artifacts(source, profile_path):
    """Generate views from a real ZAL source and the exact observation profile."""
    import behavior
    import observation_game
    from game_spec import unique, reject_number
    with source.open(encoding="ascii") as stream:
        text = stream.read(behavior.MAX_SOURCE + 1)
    with profile_path.open(encoding="utf-8") as stream:
        profile_text = stream.read(65537)
    _need(len(profile_text.encode()) <= 65536, "diagram profile byte bound")
    model = behavior.parse(text)
    value = json.loads(profile_text, object_pairs_hook=unique,
                       parse_float=reject_number, parse_constant=reject_number)
    profile = observation_game.parse_profile(value, model)
    report = observation_game.check(model, profile)
    _need(report["status"] != "UNKNOWN", "diagram requires complete checked model")
    compiled = report["compilation"]
    labels = {}
    for i, node in enumerate(compiled["nodes"]):
        label = str(node.get("state", node.get("kind", f"node {i}")))
        if node.get("event") is not None:
            label += ": " + str(node["event"]) + " reply"
        if profile.max_cost is not None and node.get("spent") is not None:
            label += " / cost " + str(node["spent"])
        labels[i] = label
    result = render(compiled["arena"], labels, report["packet"])
    result["metadata"].update(model_revision=model.revision, profile=value,
                                profile_digest=_digest(value), compilation=compiled)
    return result, model.render(True)


def render_relational(model, profile, report=None):
    """Draw a rechecked pair of execution witnesses, never an asserted verdict."""
    import relational_observation as relational
    fresh = relational.check(model, profile)
    if report is not None:
        _need(_canonical(report) == _canonical(fresh), "relational diagram report mismatch")
    report = fresh
    _need(report["status"] == "COUNTEREXAMPLE", "relational diagram requires a counterexample")
    witness = report["counterexample"]
    steps = witness["steps"]
    _need(len(steps) < MAX_NODES // 2, "relational diagram node bound (64)")
    observed = set(profile.observed_variables)
    rows = [witness["initial"]]
    rows.extend({side: dict(control=step[side]["outcome"]["state"],
                            variables=step[side]["outcome"]["variables"])
                 for side in ("left", "right")} for step in steps)
    variable_count = max(len(row[side]["variables"]) for row in rows for side in ("left", "right"))
    card_height = 76 + variable_count * 24
    width, gap, top = 1080, 108, 142
    height = top + len(rows) * (card_height + gap) + 70
    out = [f'<svg xmlns="http://www.w3.org/2000/svg" width="{width}" height="{height}" viewBox="0 0 {width} {height}" role="img" aria-labelledby="title description">',
           '<title id="title">ZAL paired execution · an observed difference</title>',
           '<desc id="description">Two concrete executions use the same event sequence and equal public inputs. Secret values differ. Each row shows the resulting state; the final observed results differ. This is one rechecked counterexample, not the entire reachable product.</desc>',
           '<style>text{font-family:ui-sans-serif,system-ui,sans-serif;fill:#172438}.title{font-size:26px;font-weight:700}.body{font-size:15px}.small{font-size:13px;fill:#41546a}.name{font-size:17px;font-weight:650}.observed{font-size:15px;font-weight:650;fill:#644899}</style>',
           '<defs><marker id="arrow" markerWidth="7" markerHeight="7" refX="6" refY="3.5" orient="auto"><path d="M0,0 L7,3.5 L0,7 z" fill="#70849b"/></marker></defs>',
           f'<rect width="{width}" height="{height}" fill="#f8fafc"/>',
           _text(36, 45, "ZAL paired execution · an observed difference", "title"),
           _text(36, 75, "Same events and public inputs; different initial secrets. Complete states are shown for explanation.", "body"),
           _text(36, 101, "Fields marked [observed], outcome class, and rejection reason belong to the declared public observation.", "small")]
    for row_number, row in enumerate(rows):
        y = top + row_number * (card_height + gap)
        is_last = row_number == len(rows) - 1
        for side, x in (("left", 44), ("right", 584)):
            state = row[side]
            border = "#bd4553" if is_last else "#718196"
            fill = "#fff2f3" if is_last else "#eef1f5"
            out.append(f'<g data-run="{side}" data-position="{row_number}"><rect x="{x}" y="{y}" width="452" height="{card_height}" rx="11" fill="{fill}" stroke="{border}" stroke-width="2"/>')
            prefix = "Initial" if row_number == 0 else f"After {steps[row_number - 1]['event']}"
            control_mark = " [observed]" if profile.observed_control else ""
            out.append(_text(x + 16, y + 27, f"{side.capitalize()} · {prefix} · {state['control']}{control_mark}", "name"))
            for j, (name, value) in enumerate(sorted(state["variables"].items())):
                suffix = " [observed]" if name in observed else ""
                out.append(_text(x + 16, y + 57 + j * 24, f"{name} = {value}{suffix}",
                                 "observed" if name in observed else "body"))
            if row_number:
                result = steps[row_number - 1][side]["outcome"]
                out.append(_text(x + 16, y + card_height - 15,
                                 "Outcome: " + result["class"] + "; reason: " + str(result["reason"]), "small"))
            else:
                out.append(_text(x + 16, y + card_height - 15, "Zero events; initial state observation checked", "small"))
            out.append('</g>')
            if row_number < len(steps):
                step = steps[row_number]
                start, end = y + card_height, y + card_height + gap
                out.append(f'<path d="M {x + 226},{start} L {x + 226},{end}" fill="none" stroke="#70849b" stroke-width="1.5" marker-end="url(#arrow)" data-run="{side}" data-step="{row_number + 1}"><title>{escape(_canonical(step[side]))}</title></path>')
                out.append(f'<rect x="{x + 20}" y="{start + 20}" width="412" height="66" rx="7" fill="#f8fafc"/>')
                out.append(_text(x + 35, start + 42, f"Event {row_number + 1}: {step['event']}", "body"))
                context = ", ".join(f"{k}={str(v).lower()}" for k, v in sorted(step[side]["context"].items())) or "none"
                for j, line in enumerate(_lines("Inputs: " + context, width=50, maximum=2)):
                    out.append(_text(x + 35, start + 62 + j * 16, line, "small"))
    y = top + len(rows) * (card_height + gap) - 58
    out.append(_text(44, y, f"First different observation after {len(steps)} event(s).", "name"))
    out.append(_text(44, y + 29, "The JSON companion retains the exact paired witness, coupling, observation profile, and model revision.", "small"))
    out.append('</svg>')
    return dict(svg="\n".join(out) + "\n",
                metadata=dict(schema=SCHEMA, kind="relational-counterexample", model_revision=model.revision,
                              profile=profile.wire(), report_digest=_digest(report), report=report,
                              scope="one rechecked synchronized finite-model witness; no native execution authority"))


def relational_artifacts(source, profile_path):
    import behavior2
    import relational_observation as relational
    with source.open(encoding="ascii") as stream:
        text = stream.read(behavior2.MAX_SOURCE + 1)
    model = behavior2.parse(text)
    value, _ = relational.load_profile(profile_path)
    profile = relational.parse_profile(value, model)
    return render_relational(model, profile), model.render(True)


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("source", type=Path)
    parser.add_argument("--profile", required=True, type=Path)
    parser.add_argument("--out", required=True, type=Path)
    parser.add_argument("--name", default="delivery")
    parser.add_argument("--mode", choices=("game", "relational"), default="game")
    parser.add_argument("--check", action="store_true", help="Refuse stale or missing generated files")
    args = parser.parse_args(argv)
    try:
        _need(args.name.isascii() and args.name and all(c.isalnum() or c in "_-" for c in args.name),
              "diagram output name")
        if args.mode == "game":
            result, english = artifacts(args.source, args.profile)
            files = {args.name + ".arena.svg": result["arena_svg"]}
            if result["knowledge_svg"] is not None:
                files[args.name + ".beliefs.svg"] = result["knowledge_svg"]
        else:
            result, english = relational_artifacts(args.source, args.profile)
            files = {args.name + ".paired.svg": result["svg"]}
        files.update({args.name + ".json": json.dumps(result["metadata"], indent=2, sort_keys=True) + "\n",
                      args.name + ".english.txt": english})
        if args.check:
            for name, contents in files.items():
                _need((args.out / name).read_text() == contents, "stale generated diagram: " + name)
        else:
            args.out.mkdir(parents=True, exist_ok=True)
            for name, contents in files.items():
                (args.out / name).write_text(contents)
        print(json.dumps(dict(status="CHECKED", schema=SCHEMA,
                              files=sorted(files), model_revision=result["metadata"]["model_revision"])))
        return 0
    except (ValueError, KeyError, TypeError, IndexError, OSError, UnicodeError) as error:
        print(json.dumps(dict(status="UNKNOWN", reason=str(error), authority="none")))
        return 2


if __name__ == "__main__":
    raise SystemExit(main())
