"""Explicit finite-ZAL application input binding; no runtime authority.

The caller supplies the complete model, source origins and human-facing metadata.
The ordinary ZAL lowerer remains unchanged unless this adapter is selected.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import sys
import unicodedata
from pathlib import Path

import behavior2
import factory

FORMAT = "zal/app-binding/1"
PRESENTATION_FORMAT = "zal/app-presentation/1"
MAX_BINDING_BYTES = 65536


def _need(test, message):
    if not test:
        raise behavior2.Refusal("app-binding: " + message)


def _object(value, keys, where):
    _need(type(value) is dict and set(value) == set(keys), where + " must have exactly the declared keys")
    return value


def _pairs(pairs):
    out = {}
    for key, value in pairs:
        _need(key not in out, "duplicate JSON key: " + key)
        out[key] = value
    return out


def _bad_number(_number):
    raise behavior2.Refusal("app-binding: nonfinite JSON number")


def _text(value, where):
    _need(type(value) is str and bool(value.strip()) and len(value) <= 300, where + " must be visible text")
    for char in value:
        code = ord(char)
        hidden = (unicodedata.category(char) in {"Cc", "Cf", "Cs", "Co", "Zl", "Zp"}
                  or code in {0x034f, 0x115f, 0x1160, 0x17b4, 0x17b5, 0x2065, 0x3164, 0xffa0}
                  or 0x180b <= code <= 0x180f or 0xfe00 <= code <= 0xfe0f
                  or 0xfff0 <= code <= 0xfff8 or 0xe0000 <= code <= 0xe0fff)
        _need(not hidden and char not in "<>", where + " contains unsupported or invisible text")
    _need(len(value.encode("utf-16-le")) // 2 <= 300, where + " exceeds the UI text bound")
    return value


def _label_unit(value, where):
    _object(value, {"label", "unit"}, where)
    return {"label": _text(value["label"], where + ".label"),
            "unit": _text(value["unit"], where + ".unit")}


def _keyed(value, names, where):
    _need(type(value) is dict and set(value) == set(names), where + " must name every declared item exactly once")
    return value


def _sha(data):
    return hashlib.sha256(data).hexdigest()


def _parse_binding(model, text):
    _need(type(text) is str, "binding must be JSON text")
    _need(len(text) <= MAX_BINDING_BYTES, "binding JSON exceeds 64 KiB")
    try:
        raw = text.encode("utf-8")
    except UnicodeError as error:
        raise behavior2.Refusal("app-binding: invalid Unicode") from error
    _need(len(raw) <= MAX_BINDING_BYTES, "binding JSON exceeds 64 KiB")
    try:
        binding = json.loads(text, object_pairs_hook=_pairs, parse_constant=_bad_number)
    except behavior2.Refusal:
        raise
    except (ValueError, RecursionError, UnicodeError) as error:
        raise behavior2.Refusal("app-binding: invalid JSON") from error
    _object(binding, {"format", "model_revision", "observations", "ui"}, "binding")
    _need(binding["format"] == FORMAT, "unsupported binding format")
    _need(type(binding["model_revision"]) is str and binding["model_revision"] == model.revision,
          "model revision does not match the source")
    declared = {**{name: "fact" for name in model.facts}, **{item.name: "input" for item in model.inputs}}
    observations = binding["observations"]
    _need(type(observations) is list and len(observations) == len(declared), "every fact and input needs one binding")
    entries = {}
    for item in observations:
        _object(item, {"name", "kind", "source", "role", "label", "unit"}, "observation")
        name = item["name"]
        _need(type(name) is str and name in declared and name not in entries, "unknown or duplicate observation")
        _need(item["kind"] == declared[name], "observation kind does not match the model")
        source, role = item["source"], item["role"]
        _need(type(source) is str and type(role) is str, "observation source and role must be text")
        _need((source == "command" and role == "intent")
              or (source == "server-context" and role in {"observation", "authorization", "identity"}),
              "intent belongs to command; server observations, authorization and identity belong to context")
        entries[name] = {"kind": declared[name], "source": source, "role": role,
                         **_label_unit({"label": item["label"], "unit": item["unit"]}, name)}
    _need(set(entries) == set(declared), "every fact and input needs one binding")
    ui_keys = {"title", "action", "control", "variables", "states", "events", "reasons"}
    if type(binding["ui"]) is dict and "presentation" in binding["ui"]:
        ui_keys.add("presentation")
    ui = _object(binding["ui"], ui_keys, "ui")
    if "presentation" in ui:
        presentation = _object(ui["presentation"], {"format", "commands"}, "ui.presentation")
        _need(type(presentation["format"]) is str and presentation["format"] == PRESENTATION_FORMAT,
              "unsupported presentation format")
        _need(type(presentation["commands"]) is str and presentation["commands"] == "grouped",
              "unsupported command presentation")
    _text(ui["title"], "ui.title")
    _label_unit(ui["action"], "ui.action")
    _label_unit(ui["control"], "ui.control")
    for item in model.variables:
        _label_unit(_keyed(ui["variables"], [v.name for v in model.variables], "ui.variables")[item.name], item.name)
    _keyed(ui["variables"], [v.name for v in model.variables], "ui.variables")
    for name in model.states:
        _text(_keyed(ui["states"], model.states, "ui.states")[name], "state " + name)
    for name in model.events:
        event = _object(_keyed(ui["events"], model.events, "ui.events")[name], {"label", "consequential"}, "event " + name)
        _text(event["label"], "event " + name)
        _need(type(event["consequential"]) is bool, "event consequential must be a Boolean")
    reasons = sorted({model.default, *[rule.reason for rule in model.rules if rule.reason]})
    for name in reasons:
        _text(_keyed(ui["reasons"], reasons, "ui.reasons")[name], "reason " + name)
    return binding, entries, raw


def _grouped_requirements(mapping):
    """Group only the presentation; retain every editable/context observation."""
    observations = sorted(mapping["bindings"].values(), key=lambda item: item["field"])
    primary = [item["path"] for item in observations if item["root"] == "command"]
    context = [item["path"] for item in observations if item["root"] == "context"]
    if mapping["unit_context_field"] is not None:
        context.append("context.102.159")
    return {"collapseExpert": True, "compact": True, "contextFields": context,
            "detailFields": [], "fixedFields": [], "format": "zenoui/requirements/2",
            "groupCommands": True, "groups": [], "maxColumns": 2,
            "maxExpandedExpertPanels": 0, "maxVisibleForms": 1,
            "primaryFields": primary, "projections": [], "quoteOverrides": {}}


def _metadata(model, binding, entries, mapping):
    ui = binding["ui"]
    fields = {"pre.100.110": dict(ui["control"]), "command.101.119": dict(ui["action"])}
    fields.update({f"pre.100.{field}": dict(ui["variables"][name]) for name, field in mapping["variable_fields"].items()})
    for name, item in entries.items():
        fields[mapping["bindings"][name]["path"]] = {"label": item["label"], "unit": item["unit"]}
    if mapping["unit_context_field"] is not None:
        fields["context.102.159"] = {"label": "Unit context", "unit": "unitless"}
    variants = {str(field): ui["states"][name] for name, field in mapping["states"].items()}
    variants.update({str(field): ui["events"][name]["label"] for name, field in mapping["events"].items()})
    commands = {str(field): dict(ui["events"][name]) for name, field in mapping["events"].items()}
    reasons = {str(field): ui["reasons"][name] for name, field in mapping["reasons"].items()}
    metadata = {"format": "zenoui/metadata/1", "title": ui["title"], "action": "command.101.119",
                "fields": fields, "variants": variants, "commands": commands, "reasons": reasons}
    if "presentation" in ui:
        metadata.update(format="zenoui/metadata/2", requirements=_grouped_requirements(mapping))
    return metadata


def lower(source, binding_text):
    """Lower a ZAL2 source and explicit binding into an owned contract file map.

    Command values are caller inputs. Server context must be authenticated and
    supplied by the host; a role label or this adapter does not authenticate it.
    """
    _need(type(source) is str, "ZAL source must be text")
    _need(len(source) <= behavior2.MAX_SOURCE, "ZAL source exceeds 64 KiB")
    try:
        model = behavior2.parse(source)
    except UnicodeError as error:
        raise behavior2.Refusal("app-binding: invalid source Unicode") from error
    binding, entries, raw_binding = _parse_binding(model, binding_text)
    origins = {name: "command" if item["source"] == "command" else "context" for name, item in entries.items()}
    files = factory.lower_machine(model, _bindings=origins)
    mapping = json.loads(files["zal-source-map.json"])
    ui = _metadata(model, binding, entries, mapping)
    ui_text = json.dumps(ui, ensure_ascii=True, indent=2) + "\n"
    source_bytes = source.encode("utf-8")
    canonical_binding = json.dumps(binding, ensure_ascii=True, sort_keys=True, separators=(",", ":")).encode("ascii")
    fields = mapping["bindings"]
    for name, item in entries.items():
        fields[name].update(kind=item["kind"], role=item["role"])
    positions = sorted((value["field"], value["path"]) for value in fields.values())
    order = ["pre.100.110", *[f"pre.100.{field}" for field in mapping["variable_fields"].values()], "command.101.119"]
    order.extend(path for _, path in positions if path.startswith("command."))
    order.extend(path for _, path in positions if path.startswith("context."))
    if mapping["unit_context_field"] is not None:
        order.append("context.102.159")
    mapping.update(input_order=order, source_sha256=_sha(source_bytes),
                   binding_sha256=_sha(raw_binding), canonical_binding_sha256=_sha(canonical_binding),
                   project_sha256=_sha(files["project.zeno"].encode("utf-8")),
                   policy_sha256=_sha(files["v2/policy.json"].encode("utf-8")),
                   ui_sha256=_sha(ui_text.encode("utf-8")),
                   reference_sha256=_sha(files["tests/zal-authority-examples.txt"].encode("utf-8")),
                   authentication="host responsibility; role labels are not authenticated identity")
    files.update({"ui.json": ui_text, "zal-original.zal": source, "zal-app-binding.json": binding_text,
                  "zal-source-map.json": json.dumps(mapping, indent=2) + "\n"})
    return files


def lower_files(source_path, binding_path):
    """Read explicit UTF-8 source and JSON files, then invoke the same binding."""
    def read(path, limit):
        with Path(path).open("rb") as handle:
            data = handle.read(limit + 1)
        _need(len(data) <= limit, "input file exceeds its byte limit")
        try:
            return data.decode("utf-8")
        except UnicodeError as error:
            raise behavior2.Refusal("app-binding: input is not UTF-8") from error
    return lower(read(source_path, behavior2.MAX_SOURCE), read(binding_path, MAX_BINDING_BYTES))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--source", type=Path, required=True)
    parser.add_argument("--binding", type=Path, required=True)
    parser.add_argument("--out", type=Path, required=True)
    args = parser.parse_args()
    try:
        files = lower_files(args.source, args.binding)
        factory.write_new(files, args.out, exact_utf8=True)
        mapping = json.loads(files["zal-source-map.json"])
        print(json.dumps({"schema": "zal/app-binding-result/1", "status": "lowered", "authority": "none",
                          "model_revision": mapping["revision"], "source_sha256": mapping["source_sha256"],
                          "binding_sha256": mapping["binding_sha256"], "project_sha256": mapping["project_sha256"],
                          "policy_sha256": mapping["policy_sha256"], "ui_sha256": mapping["ui_sha256"]}, sort_keys=True))
        return 0
    except (ValueError, OSError) as error:
        print(json.dumps({"schema": "zal/app-binding-result/1", "status": "refused",
                          "authority": "none", "detail": str(error)[:1000]}, sort_keys=True), file=sys.stderr)
        return 2


if __name__ == "__main__":
    raise SystemExit(main())
