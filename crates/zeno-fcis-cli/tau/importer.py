#!/usr/bin/env python3
"""Translate a bounded, pointwise Tau ladder; never evaluate a policy in Python."""
from __future__ import annotations

import argparse
import hashlib
import json
import re
import sys

MAX_SOURCE = 1 << 20
MAX_TOKENS = 20000
MAX_WIDTH = 16
MAX_BRANCHES = 128
ALLOWANCE_PROFILE = "allowance-study-bv3"
TOKEN = re.compile(r"\s*(?:(:=|!=|<=|>=|&&|\|\||->)|([A-Za-z_][A-Za-z_0-9]*)|([0-9]+)|([{}\[\]().,:=+!?<>-]))")
PRECEDENCE = {"->": 1, "||": 2, "&&": 3, "=": 4, "!=": 4,
              "<": 5, "<=": 5, ">": 5, ">=": 5, "+": 6, "-": 6}


class Refused(ValueError):
    pass


def unique_object(entries):
    result = {}
    for key, value in entries:
        if key in result:
            raise Refused(f"duplicate JSON key {key}")
        result[key] = value
    return result


def tokens(source):
    if len(source.encode()) > MAX_SOURCE:
        raise Refused("Tau source exceeds 1 MiB")
    source = "\n".join(line.split("#", 1)[0] for line in source.splitlines()).strip()
    result, index = [], 0
    while index < len(source):
        match = TOKEN.match(source, index)
        if not match:
            raise Refused(f"unsupported Tau syntax at character {index}")
        result.append(next(group for group in match.groups() if group is not None))
        index = match.end()
        if len(result) > MAX_TOKENS:
            raise Refused("too many Tau tokens")
    return result


class Parser:
    def __init__(self, source, scaling, profile=None):
        self.tokens, self.index = tokens(source), 0
        self.aliases, self.streams, self.widths = {}, {}, set()
        self.scaling = scaling
        self.profile = profile

    def peek(self):
        return self.tokens[self.index] if self.index < len(self.tokens) else None

    def take(self, expected=None):
        found = self.peek()
        if found is None or expected is not None and found != expected:
            raise Refused(f"expected {expected or 'a token'}, found {found or 'end of source'}")
        self.index += 1
        return found

    def identifier(self):
        found = self.take()
        if not re.fullmatch(r"[A-Za-z_][A-Za-z_0-9]*", found):
            raise Refused("expected an identifier")
        return found

    def number(self):
        value = self.take()
        if not value.isdigit() or len(value) > 80:
            raise Refused("expected a bounded decimal integer")
        return int(value)

    def width(self):
        self.take("bv")
        self.take("[")
        original = self.number()
        self.take("]")
        if not 1 <= original <= 256:
            raise Refused("source bit width must be in 1..256")
        self.widths.add(original)
        effective = self.scaling.get(original, original)
        if not 1 <= effective <= MAX_WIDTH or effective > original:
            raise Refused("bit widths above 16 need an explicit narrowing --scale-width original:new")
        return original, effective

    def type(self):
        if self.peek() == "bv":
            return self.width()
        if self.peek() == "{":
            self.take("{")
            fields = {}
            while True:
                name = self.identifier()
                self.take(":")
                value = self.type()
                if name in fields or not isinstance(value, tuple):
                    raise Refused("record fields must be unique scalar bitvectors")
                fields[name] = value
                if self.peek() != ",":
                    break
                self.take(",")
            self.take("}")
            if not 1 <= len(fields) <= 16:
                raise Refused("records must contain 1..16 scalar fields")
            return fields
        name = self.identifier()
        if name not in self.aliases:
            raise Refused(f"unknown or recursive type {name}")
        return self.aliases[name]

    def expression(self, level=0, depth=0):
        if depth > 64:
            raise Refused("expression nests beyond 64 levels")
        token = self.take()
        if token == "(":
            left = self.expression(0, depth + 1)
            self.take(")")
        elif token == "!":
            left = ("not", self.expression(7, depth + 1))
        elif token in ("T", "F"):
            left = ("bool", token == "T")
        elif token == "{":
            value = self.number()
            self.take("}")
            self.take(":")
            original, effective = self.width()
            if value >= 1 << original:
                raise Refused("constant is outside its written bit width")
            if original != effective and value == (1 << original) - 1:
                value = (1 << effective) - 1
            if value >= 1 << effective:
                raise Refused("constant cannot be represented after explicit width scaling")
            left = ("int", value, original, effective)
        elif re.fullmatch(r"[io][1-9][0-9]*", token):
            self.take("[")
            self.take("t")
            self.take("]")
            name = token
            if self.peek() == "." and self.index + 1 < len(self.tokens) and re.fullmatch(r"[A-Za-z_][A-Za-z_0-9]*", self.tokens[self.index + 1]):
                # A final statement period is consumed only after a complete
                # formula; here a period must introduce a record field.
                self.take(".")
                name += "." + self.identifier()
            left = ("name", name)
        else:
            raise Refused(f"unsupported expression token {token}")
        while self.peek() in PRECEDENCE and PRECEDENCE[self.peek()] >= level:
            operator = self.take()
            precedence = PRECEDENCE[operator]
            right = self.expression(precedence if operator == "->" else precedence + 1, depth + 1)
            left = ("binary", operator, left, right)
        if level == 0 and self.peek() == "?":
            self.take("?")
            yes = self.expression(0, depth + 1)
            self.take(":")
            no = self.expression(0, depth + 1)
            left = ("choose", left, yes, no)
        return left

    def parse(self):
        while self.peek() == "type":
            self.take("type")
            name = self.identifier()
            self.take("=")
            value = self.type()
            self.take(".")
            if name in self.aliases:
                raise Refused(f"duplicate type {name}")
            self.aliases[name] = value
        while self.peek() != "always":
            name = self.identifier()
            allowed = {"i1", "i2", "i3", "o1", "o2"}
            if self.profile == ALLOWANCE_PROFILE:
                allowed |= {"i4", "i5", "i6", "i7", "o3", "o4"}
            if name not in allowed or name in self.streams:
                raise Refused("exactly i1, i2, i3, o1 and o2 must be declared once")
            self.take(":")
            value = self.type()
            self.take(":=")
            self.take("in" if name.startswith("i") else "out")
            self.take("console")
            self.take(".")
            self.streams[name] = value
        self.take("always")
        formula = self.expression()
        if self.peek() == ".":
            self.take(".")
        elif self.profile != ALLOWANCE_PROFILE or self.streams:
            self.take(".")
        if self.peek() is not None:
            raise Refused("unsupported statements after the pointwise ladder")
        if self.profile == ALLOWANCE_PROFILE and not self.streams:
            # The bare profile has one explicitly written uniform bv3/bv8
            # algebra. Stream roles are fixed, never inferred from free names.
            if len(self.widths) != 1 or next(iter(self.widths)) not in (3, 8):
                raise Refused("bare allowance streams require one bv3 or bv8 width")
            width = next(iter(self.widths))
            self.streams = {name: (width, self.scaling.get(width, width))
                            for name in [*(f"i{i}" for i in range(1, 8)), *(f"o{i}" for i in range(1, 5))]}
        if self.profile == ALLOWANCE_PROFILE and set(self.streams) == {
                *(f"i{i}" for i in range(1, 8)), *(f"o{i}" for i in range(1, 5))}:
            if any(not isinstance(value, tuple) for value in self.streams.values()):
                raise Refused("flat allowance streams must be scalar bitvectors")
        else:
            if set(self.streams) != {"i1", "i2", "i3", "o1", "o2"}:
                raise Refused("all five record-layout streams must be declared")
            if not isinstance(self.streams["i1"], dict) or self.streams["o1"] != self.streams["i1"]:
                raise Refused("o1 must be the same complete scalar record as i1")
            if not isinstance(self.streams["o2"], tuple):
                raise Refused("o2 must be a scalar 0/1 decision bitvector")
        if any(width not in self.widths for width in self.scaling):
            raise Refused("width scaling names a width absent from the source")
        return formula


def leaves(streams):
    result = {}
    for name, value in streams.items():
        if isinstance(value, dict):
            for field, width in value.items():
                result[name + "." + field] = width
        else:
            result[name] = value
    return result


def typed(expression, names):
    tag, *args = expression
    if tag == "bool":
        return "bool"
    if tag == "int":
        return args[-1]
    if tag == "name":
        if args[0] not in names:
            raise Refused(f"undeclared scalar projection {args[0]}")
        return names[args[0]][1]
    if tag == "not":
        if typed(args[0], names) != "bool":
            raise Refused("logical negation requires a formula")
        return "bool"
    if tag == "choose":
        condition, yes, no = args
        if typed(condition, names) != "bool" or typed(yes, names) != typed(no, names):
            raise Refused("conditional operands have inconsistent types")
        if typed(yes, names) != "bool":
            raise Refused("numeric conditionals are outside the pointwise source profile")
        return typed(yes, names)
    operator, left, right = args
    a, b = typed(left, names), typed(right, names)
    if a != b:
        raise Refused("mixed-width expressions are not supported")
    if operator in ("&&", "||", "->"):
        if a != "bool":
            raise Refused("logical operations require formulas")
        return "bool"
    if a == "bool":
        raise Refused("bitvector comparisons/arithmetic require bitvector operands")
    return a if operator in ("+", "-") else "bool"


def used_names(expression):
    tag, *args = expression
    if tag == "name":
        return {args[0]}
    if tag in ("int", "bool"):
        return set()
    return set().union(*(used_names(arg) for arg in args if isinstance(arg, (tuple, list))))


def conjunction(items):
    result = ("bool", True)
    for item in items:
        result = ("binary", "&&", result, item)
    return result


def branches(formula, path=()):
    if formula[0] == "choose":
        _, guard, yes, no = formula
        if any(name.startswith("o") for name in used_names(guard)):
            raise Refused("a ladder guard cannot depend on outputs")
        result = branches(yes, path + (guard,)) + branches(no, path + (("not", guard),))
        if len(result) > MAX_BRANCHES:
            raise Refused("too many ladder branches")
        return result
    return [(conjunction(path), formula)]


def assignments(formula):
    if formula[0:2] == ("binary", "&&"):
        left, right = assignments(formula[2]), assignments(formula[3])
        if set(left) & set(right):
            raise Refused("duplicate output assignments")
        return {**left, **right}
    if formula[0:2] != ("binary", "=") or formula[2][0] != "name":
        raise Refused("every branch must be a conjunction of output equalities")
    name, value = formula[2][1], formula[3]
    if not name.startswith("o") or any(ref.startswith("o") for ref in used_names(value)):
        raise Refused("only output = input-expression assignments are supported")
    return {name: value}


def native(expression, names, mapping):
    tag, *args = expression
    if tag == "bool":
        return "true" if args[0] else "false"
    if tag == "int":
        return str(args[0])
    if tag == "name":
        return mapping[args[0]]["native"]
    if tag == "not":
        return f"!({native(args[0], names, mapping)})"
    if tag == "choose":
        return "choose(" + ", ".join(native(item, names, mapping) for item in args) + ")"
    operator, left, right = args
    a, b = native(left, names, mapping), native(right, names, mapping)
    if operator == "+":
        modulus = 1 << typed(left, names)
        return f"choose(({a}) + ({b}) >= {modulus}, ({a}) + ({b}) - {modulus}, ({a}) + ({b}))"
    if operator == "-":
        modulus = 1 << typed(left, names)
        return f"choose(({a}) >= ({b}), ({a}) - ({b}), ({a}) - ({b}) + {modulus})"
    operator = "==" if operator == "=" else operator
    return f"(({a}) {operator} ({b}))"


def and_parts(expression):
    if expression[:2] == ("binary", "&&"):
        return and_parts(expression[2]) + and_parts(expression[3])
    return [expression]


def allowance_spec(source, scaling, domains, parser, formula, names):
    """Source-derived decisions with explicit, separately supplied annotations.

    The matched guards imply that every selected unsigned successor is in
    0..7 without wrap. Checked native +/- can therefore lower these exact ASTs.
    This theorem does not extend to an unmatched guard or to a wider domain.
    """
    roles = ["available", "reserved", "spent", "kind", "amount", "actual", "authorized"]
    if isinstance(parser.streams["i1"], dict):
        refs = ["i1.available", "i1.reserved", "i1.spent", "i2.kind", "i2.amount", "i2.actual", "i3"]
        outputs, decision = ["o1.available", "o1.reserved", "o1.spent"], "o2"
    else:
        refs = [f"i{i}" for i in range(1, 8)]
        outputs, decision = ["o1", "o2", "o3"], "o4"
    if set(names) != set(refs + outputs + [decision]):
        raise Refused("allowance profile requires exactly seven inputs and four scalar outputs")
    if scaling not in ({}, {256: 3}):
        raise Refused("allowance profile permits only explicit 256:3 width scaling")
    amount_refs = refs[:3] + refs[4:6] + outputs
    if names[refs[0]][0] not in (3, 8, 256) or any(names[ref][0] != names[refs[0]][0] for ref in amount_refs):
        raise Refused("allowance amounts require one original bv3, bv8 or explicitly scaled bv256 algebra")
    if any(names[ref][1] not in (3, 8) for ref in names):
        raise Refused("allowance profile supports effective widths 3 or 8 only")
    amount_width = names[refs[0]][1]
    if any(names[ref][1] != amount_width for ref in amount_refs):
        raise Refused("all allowance amounts must have the same effective width")
    if parser.widths - {original for original, _ in names.values()}:
        raise Refused("allowance profile contains an unused unsupported width")
    to_role = dict(zip(refs, roles))

    def shape(node):
        tag, *args = node
        if tag == "name":
            return (tag, to_role.get(args[0], args[0]))
        if tag == "int":
            return (tag, args[0])
        return (tag, *(shape(arg) if isinstance(arg, tuple) else arg for arg in args))

    def n(role): return ("name", role)
    def c(value): return ("int", value)
    def b(op, left, right): return ("binary", op, left, right)

    a, r, s, k, amt, actual, auth = map(n, roles)
    total = b("+", b("+", a, r), s)
    global_shape = [b("=", auth, c(1)), b(">", amt, c(0)),
                    b("<=", a, b("-", c(7), r)),
                    b("<=", b("+", a, r), b("-", c(7), s))]
    if formula[0] != "choose" or [shape(part) for part in and_parts(formula[1])] != global_shape:
        raise Refused("allowance profile requires the exact authority, positive-amount and no-wrap ledger guards")
    expected_outputs = set(outputs + [decision])

    def output_values(leaf, accepting):
        values = assignments(leaf)
        if set(values) != expected_outputs or shape(values[decision]) != c(int(accepting)):
            raise Refused("allowance leaves must completely assign their literal decision and successor")
        return values

    reject = output_values(formula[3], False)
    if any(reject[output] != ("name", ref) for ref, output in zip(refs, outputs)):
        raise Refused("allowance rejection must copy every old state field")
    guards, posts, ladder = [], [], formula[2]
    expected_posts = [[b("+", a, amt), r, s],
                      [b("-", a, amt), b("+", r, amt), s],
                      [b("+", a, b("-", amt, actual)), b("-", r, amt), b("+", s, actual)],
                      [b("+", a, amt), b("-", r, amt), s]]
    tail_guards = [[b("=", actual, c(0)), b("<=", total, b("-", c(7), amt))],
                   [b("=", actual, c(0)), b(">=", a, amt)],
                   [b("<=", actual, amt), b(">=", r, amt)],
                   [b("=", actual, c(0)), b(">=", r, amt)]]
    source_base = None
    for index in range(4):
        if ladder[0] != "choose":
            raise Refused("allowance profile requires four ordered kind branches")
        parts = and_parts(ladder[1])
        first = shape(parts[0])
        if index == 0:
            for candidate in (0, 140):
                if first == b("=", k, c(candidate)):
                    source_base = candidate
            if source_base is None:
                raise Refused("allowance kind codes must be 0..3 or 140..143")
        expected = [b("=", k, c(source_base + index)), *tail_guards[index]]
        if [shape(part) for part in parts] != expected:
            raise Refused("allowance kind guard differs from the supported no-wrap source AST")
        values = output_values(ladder[2], True)
        if [shape(values[output]) for output in outputs] != expected_posts[index]:
            raise Refused("allowance successor differs from its supported source AST")
        guards.append(parts)
        posts.append([values[output] for output in outputs])
        ladder = ladder[3]
    if output_values(ladder, False) != reject:
        raise Refused("all allowance rejecting leaves must have the same complete old-state observation")
    bounds = [(0, 7)] * 3 + [(source_base, source_base + 3), (0, 7), (0, 7), (0, 1)]
    finite = dict(zip(refs, bounds))
    if set(domains) - set(refs) or any(tuple(value) != finite[ref] for ref, value in domains.items()):
        raise Refused("allowance profile domain overrides must equal its declared finite input domains")
    if any(high >= 1 << names[ref][1] for ref, (_, high) in finite.items()):
        raise Refused("allowance finite code does not fit its source width")

    # Render only ASTs admitted above. No policy is evaluated here.
    def lower(node, parent=0, right=False):
        tag, *args = node
        if tag == "name": return to_role[args[0]]
        if tag == "int": return "max" if args[0] == 7 else str(args[0])
        if tag != "binary": raise Refused("unmatched allowance expression")
        op, left, rhs = args
        if shape(left) == k and op == "=":
            return f"kind == {140 + rhs[1] - source_base}"
        precedence = PRECEDENCE[op]
        text = f"{lower(left, precedence)} {'==' if op == '=' else op} {lower(rhs, precedence, True)}"
        return f"({text})" if precedence < parent or right and precedence == parent else text

    def joined(parts): return " && ".join(lower(part) for part in parts)
    global_parts = and_parts(formula[1])
    cases = []
    def add(when, reason=None, post=None):
        cases.append({"rule": f"allowance source branch {len(cases)}", "when": when,
                      "class": "Accept" if post is not None else "Reject", "reason": reason,
                      "post": {} if post is None else {str(110 + i): lower(value) for i, value in enumerate(post)}, "outbox": []})
    add("!" + lower(global_parts[0][2]), 200)
    add(f"{lower(global_parts[1][2])} == {lower(global_parts[1][3])}", 201)
    add("!(" + joined(global_parts[2:]) + ")", 202)
    for index in range(4):
        # Cancel's kind follows from the closed four-value enum and the three
        # preceding kind accept/reject pairs, not from an open integer range.
        add(joined(guards[index] if index < 3 else guards[index][1:]), post=posts[index])
        add(lower(guards[index][0]) if index < 3 else "true", 203 + index)
    roots = [100] * 3 + [101] * 3 + [102]
    fields = [110, 111, 112, 120, 121, 122, 130]
    mapping = {}
    for i, (role, ref, root, field, (low, high)) in enumerate(zip(roles, refs, roots, fields, bounds)):
        original, width = names[ref]
        row = {"native": f"{'pre' if i < 3 else 'command' if i < 6 else 'context'}.{root}.{field}",
               "field": field, "original_width": original, "width": width,
               "min": low, "max": high, "native_offset": 140 - source_base if i == 3 else 0}
        if i < 3: row["output"] = outputs[i]
        mapping[ref] = row
    project = ["zeno 1;", "project 1 allowance;", "namespace 2 tau_ladder;",
               "type 100 state Ledger;", "type 101 command LedgerCommand;", "type 102 context Authority;",
               "type 103 int Amount in 0..=7;", "type 104 data Kind;", "type 105 bool Flag;"]
    for role, root, field in zip(roles, roots, fields):
        project.append(f"field {field} {root} {role} {104 if role == 'kind' else 105 if role == 'authorized' else 103};")
    project += [f"variant {140 + i} 104 {name} none;" for i, name in enumerate(["Grant", "Reserve", "Finalize", "Cancel"])]
    reason_names = ["not_authorized", "zero_amount", "ledger_out_of_bounds", "grant_refused", "reserve_refused", "finalize_refused", "cancel_refused"]
    project += [f"reason {200 + i} {name} precedence {i};" for i, name in enumerate(reason_names)]
    project += ["component 400 ledger {", "  owns 100;", "  reads pre.100;", "  writes post.100;",
                "  contexts context.102;", "  budget steps 64;", "}", "merge [400];"]
    pre = [mapping[ref]["native"] for ref in refs[:3]]
    post = [f"post.100.{mapping[ref]['field']}" for ref in refs[:3]]
    kind, amount, actual, auth = [mapping[ref]["native"] for ref in refs[3:]]
    before, after = " + ".join(pre), " + ".join(post)
    project += [f"law 500 ledger_within_range on commit, genesis = {after} <= 7;",
                f"law 501 atoms_conserved on accept = ({kind} == 140 -> {after} == {before} + {amount}) && ({kind} != 140 -> {after} == {before});",
                f"law 502 authorized_and_monotone on accept = {auth} == 1 && {post[2]} >= {pre[2]};",
                f"law 503 finalize_spends_actual on accept = {kind} == 142 -> {post[2]} == {pre[2]} + {actual} && {post[1]} + {amount} == {pre[1]};"]
    rules = {"schema": "zeno-fcis/template-declarative-policy/2", "template": "allowance",
             "roots": {"state": 100, "command": 101, "context": 102}, "leaf_bindings": {"105": ["Bool"]},
             "variables": {**{role: mapping[ref]["native"] for role, ref in zip(roles, refs)}, "max": "7"},
             "cases": cases, "genesis": {str(field): 0 for field in fields[:3]}, "framework_reject_law": 509,
             "law_kinds": {"500": "StateInvariant", "501": "AssetConservation", "502": "AuthoritySubjectRecipient", "503": "DebitCreditEffectEquality"}}
    metadata = {"schema": "zeno-fcis/tau-import/1", "profile": ALLOWANCE_PROFILE, "source": source,
                "source_sha256": hashlib.sha256(source.encode()).hexdigest(),
                "scaling": {str(old): new for old, new in scaling.items()}, "domains": {ref: list(value) for ref, value in domains.items()},
                "finite_domain": {ref: list(value) for ref, value in finite.items()}, "mapping": mapping, "decision_output": decision,
                "reference": {"formula": json.loads(json.dumps(formula)), "widths": {ref: value[1] for ref, value in names.items()}},
                "scope": "explicitly-scaled" if scaling else "original-width-finite",
                "added_semantics": ["zero genesis", "ordered diagnostic reasons 200..206: " + ", ".join(reason_names),
                                    "ledger bound, conservation, supplied authority and monotone spending, exact finalize accounting laws"],
                "assumptions": ["explicit allowance-study-bv3 annotation profile", "inputs restricted to the recorded finite domains",
                                "context input is supplied by the host; Tau does not authenticate it"],
                "observations": ["decision bit", "complete state after accept", "old state on reject"],
                "not_claimed": ["authentication", "settlement", "temporal controller", "unscaled equivalence when scaled", "equivalence outside the finite profile"]}
    return {"project": "\n".join(project) + "\n", "rules": json.dumps(rules, indent=2) + "\n", "metadata": metadata}


def import_spec(source, scaling, domains, profile=None):
    if profile not in (None, ALLOWANCE_PROFILE):
        raise Refused("unknown Tau import profile")
    parser = Parser(source, scaling, profile)
    formula = parser.parse()
    names = leaves(parser.streams)
    # Narrowing must not make an ill-typed original expression admissible.
    # The parser retains each written width alongside its effective width.
    def original_types(node):
        if node[0] == "int": return (*node[:3], node[2])
        return tuple(original_types(arg) if isinstance(arg, tuple) else arg for arg in node)
    if typed(original_types(formula), {ref: (original, original) for ref, (original, _) in names.items()}) != "bool":
        raise Refused("the original ladder must be a formula")
    if typed(formula, names) != "bool":
        raise Refused("the ladder must be a formula")
    if set(domains) - set(names) or any(name.startswith("o") for name in domains):
        raise Refused("input-domain limits must name declared input scalars")
    if profile == ALLOWANCE_PROFILE:
        return allowance_spec(source, scaling, domains, parser, formula, names)
    project = ["zeno 1;", "project 1 tau_ladder;", "namespace 2 tau_ladder;",
               "type 100 state Ledger;", "type 101 command Command;", "type 102 context Context;"]
    mapping, bindings, next_type = {}, {}, 1000
    for stream, root, prefix, base in (("i1", 100, "pre", 120), ("i2", 101, "command", 140), ("i3", 102, "context", 160)):
        value = parser.streams[stream]
        fields = list(value) if isinstance(value, dict) else [None]
        for offset, field in enumerate(fields):
            ref = stream if field is None else stream + "." + field
            original, width = names[ref]
            low, high = domains.get(ref, (0, (1 << width) - 1))
            if not 0 <= low <= high < 1 << width or low != 0:
                raise Refused("input domains must be nonempty unsigned ranges containing zero")
            if stream == "i1" and high != (1 << width) - 1:
                raise Refused("state inputs must retain the complete bitvector domain so successors preserve source semantics")
            if field is None:
                bindings[str(root)] = ["I128", low, high]
                native_name, field_id = f"{prefix}.{root}", None
            else:
                field_id = base + offset
                native_name = f"{prefix}.{root}.{field_id}"
                project += [f"type {next_type} int {stream}_{field}_value in {low}..={high};",
                            f"field {field_id} {root} {stream}_{field} {next_type};"]
                next_type += 1
            mapping[ref] = {"native": native_name, "field": field_id,
                            "original_width": original, "width": width, "min": low, "max": high}
    state = [(name, row) for name, row in mapping.items() if name.startswith("i1.")]
    cases, reasons = [], []
    for index, (guard, leaf) in enumerate(branches(formula)):
        values = assignments(leaf)
        if set(values) != {name for name in names if name.startswith("o")}:
            raise Refused("every branch must assign every state output and o2 exactly once")
        status = values["o2"]
        if status[0] != "int" or status[1] not in (0, 1):
            raise Refused("o2 must be assigned the literal decision 0 or 1")
        accepting = status[1] == 1
        post = {}
        for name, row in state:
            output = "o1." + name.split(".", 1)[1]
            if not accepting and values[output] != ("name", name):
                raise Refused("a decision-zero branch must preserve every old state field")
            if accepting:
                post[str(row["field"])] = native(values[output], names, mapping)
        reason = None if accepting else 200 + len(reasons)
        if reason is not None:
            reasons.append(reason)
        expression = native(guard, names, mapping)
        # Earlier explicit path guards cover the domain. The final native
        # case is the unconditional fallback required by the existing compiler.
        cases.append({"rule": f"Tau branch {index}", "when": expression,
                      "class": "Accept" if accepting else "Reject", "reason": reason,
                      "post": post, "outbox": []})
    cases[-1]["when"] = "true"
    if any(len(case["when"]) > 65536 or any(len(value) > 65536 for value in case["post"].values()) for case in cases):
        raise Refused("translated expression exceeds its source cap")
    project += [f"reason {reason} tau_reject_{index} precedence {index};" for index, reason in enumerate(reasons)]
    project += ["component 400 ladder {", "  owns 100;", "  reads pre.100;", "  writes post.100;",
                "  contexts context.102;", "  budget steps 4096;", "}", "merge [400];"]
    invariant = " && ".join(f"post.100.{row['field']} >= 0 && post.100.{row['field']} <= {row['max']}" for _, row in state)
    project += [f"law 500 unsigned_state_domains on commit, genesis = {invariant};"]
    rules = {"schema": "zeno-fcis/template-declarative-policy/2", "template": "tau-ladder",
             "roots": {"state": 100, "command": 101, "context": 102}, "leaf_bindings": bindings,
             "variables": {}, "cases": cases,
             "genesis": {str(row["field"]): 0 for _, row in state}, "law_kinds": {"500": "StateInvariant"}}
    metadata = {"schema": "zeno-fcis/tau-import/1", "source": source,
                "source_sha256": hashlib.sha256(source.encode()).hexdigest(),
                "scaling": {str(old): new for old, new in scaling.items()},
                "domains": {name: list(bounds) for name, bounds in domains.items()}, "mapping": mapping,
                "reference": {"formula": json.loads(json.dumps(formula)), "widths": {name: width[1] for name, width in names.items()}},
                "scope": "explicitly-scaled" if scaling else "original-width-finite",
                "added_semantics": ["zero genesis", "diagnostic reject reasons", "native state-domain law"],
                "assumptions": ["context input is supplied by the host; Tau does not authenticate it"],
                "observations": ["decision bit", "complete state after accept", "old state on reject"],
                "not_claimed": ["authentication", "settlement", "temporal controller", "unscaled equivalence when scaled"]}
    return {"project": "\n".join(project) + "\n", "rules": json.dumps(rules, indent=2) + "\n", "metadata": metadata}


def literal(value, width, language):
    return f"{{{value}}}:bv[{width}]" if language == "tau" else f"(_ bv{value} {width})"


def render(expression, names, values, language):
    tag, *args = expression
    if tag == "bool":
        return ("T" if args[0] else "F") if language == "tau" else ("true" if args[0] else "false")
    if tag == "int":
        return literal(args[0], args[-1], language)
    if tag == "name":
        value = values[args[0]]
        if isinstance(value, (tuple, list)):
            return render(value, names, values, language)
        return literal(value, names[args[0]][1], language)
    if tag == "not":
        inner = render(args[0], names, values, language)
        return f"!({inner})" if language == "tau" else f"(not {inner})"
    if tag == "choose":
        condition, yes, no = [render(item, names, values, language) for item in args]
        return f"(({condition}) ? ({yes}) : ({no}))" if language == "tau" else f"(ite {condition} {yes} {no})"
    operator, left, right = args
    a, b = render(left, names, values, language), render(right, names, values, language)
    if language == "tau":
        return f"(({a}) {operator} ({b}))"
    if operator == "!=":
        return f"(not (= {a} {b}))"
    smt = {"+": "bvadd", "-": "bvsub", "<": "bvult", "<=": "bvule", ">": "bvugt", ">=": "bvuge",
           "=": "=", "&&": "and", "||": "or", "->": "=>"}[operator]
    return f"({smt} {a} {b})"


def correspondence(metadata, decisions):
    if metadata.get("schema") != "zeno-fcis/tau-import/1" or hashlib.sha256(metadata["source"].encode()).hexdigest() != metadata["source_sha256"]:
        raise Refused("import source identity mismatch")
    scaling = {int(old): new for old, new in metadata["scaling"].items()}
    profile = metadata.get("profile")
    rebuilt = import_spec(metadata["source"], scaling, {name: tuple(bounds) for name, bounds in metadata["domains"].items()}, profile)
    if rebuilt["metadata"] != metadata:
        raise Refused("import metadata does not describe its parsed source")
    parser = Parser(metadata["source"], scaling, profile)
    formula, names = parser.parse(), leaves(parser.streams)
    result = []
    decision_output = metadata.get("decision_output", "o2")
    for index, decision in enumerate(decisions):
        values = {}
        for ref, row in metadata["mapping"].items():
            if row["native"] not in decision["inputs"]:
                raise Refused("native decision omits a source input")
            value = int(decision["inputs"][row["native"]]) - row.get("native_offset", 0)
            if not row["min"] <= value <= row["max"]:
                raise Refused("native decision input is outside the import profile")
            values[ref] = value
        if decision["class"] not in ("Accept", "Reject"):
            raise Refused("import correspondence knows only Accept and Reject")
        values[decision_output] = int(decision["class"] == "Accept")
        for ref, row in metadata["mapping"].items():
            output = row.get("output") if profile else "o1." + ref.split(".", 1)[1] if ref.startswith("i1.") else None
            if output is not None:
                values[output] = int(decision["post"][str(row["field"])] if values[decision_output] else values[ref])
                if not 0 <= values[output] < 1 << names[output][1]:
                    raise Refused("native successor is outside the source bitvector domain")
        if not values[decision_output] and (decision["post"] or decision["outbox"]):
            raise Refused("native reject has a successor or an outbox")
        if values[decision_output] and decision["outbox"]:
            raise Refused("source ladder has no delivery semantics")
        for suffix, output_values, expected in (("", values, "T"), ("-wrong-bit", {**values, decision_output: 1 - values[decision_output]}, "F")):
            if suffix and index >= 4:
                continue
            tau = render(formula, names, output_values, "tau")
            smt = render(formula, names, output_values, "smt")
            result.append({"id": f"source-decision-{index}{suffix}", "kind": "import-control" if suffix else "import-decision",
                           "expected": expected, "tau": tau,
                           "smt": f"(set-logic QF_BV)\n(assert (not {smt}))\n(check-sat)\n"})
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--correspondence", action="store_true")
    parser.add_argument("--profile", choices=[ALLOWANCE_PROFILE])
    parser.add_argument("--scale-width", action="append", default=[])
    parser.add_argument("--domain", action="append", default=[])
    args = parser.parse_args()
    if args.correspondence:
        raw = sys.stdin.read(MAX_SOURCE * 32 + 1)
        if len(raw.encode()) > MAX_SOURCE * 32:
            raise Refused("correspondence input exceeds 32 MiB")
        value = json.loads(raw, object_pairs_hook=unique_object)
        metadata = value["metadata"]
        result = import_spec(metadata["source"], {int(old): new for old, new in metadata["scaling"].items()},
                             {name: tuple(bounds) for name, bounds in metadata["domains"].items()}, metadata.get("profile"))
        result["queries"] = correspondence(metadata, value["decisions"])
    else:
        scaling, domains = {}, {}
        for text in args.scale_width:
            match = re.fullmatch(r"([0-9]+):([0-9]+)", text)
            if not match or int(match[1]) in scaling:
                raise Refused("width scale must be a unique original:new pair")
            scaling[int(match[1])] = int(match[2])
        for text in args.domain:
            match = re.fullmatch(r"(i[1-7](?:\.[A-Za-z_][A-Za-z_0-9]*)?):([0-9]+):([0-9]+)", text)
            if not match or match[1] in domains:
                raise Refused("domain must be a unique input:min:max triple")
            domains[match[1]] = int(match[2]), int(match[3])
        result = import_spec(sys.stdin.read(MAX_SOURCE + 1), scaling, domains, args.profile)
    sys.stdout.write(json.dumps(result, sort_keys=True) + "\n")


if __name__ == "__main__":
    try:
        import resource
        for key, target in ((resource.RLIMIT_AS, 256 << 20), (resource.RLIMIT_CPU, 10),
                            (resource.RLIMIT_CORE, 0)):
            inherited = resource.getrlimit(key)
            expected = tuple(target if value == resource.RLIM_INFINITY else min(value, target)
                             for value in inherited)
            resource.setrlimit(key, expected)
            if resource.getrlimit(key) != expected:
                raise RuntimeError("rlimit-readback")
        main()
    except (Refused, KeyError, TypeError, ValueError, RecursionError) as error:
        print(f"Tau import refused: {error}", file=sys.stderr)
        raise SystemExit(1)
