#!/usr/bin/env python3
"""Produce the coupled applications' component references after a certificate refresh.

Each component of a coupled application names a family certificate in its
core-instance.json (SHA-256 of the certificate bytes) and in the provenance
comment of its project.zeno; the application's composition.json pins each
core-instance.json. All three use the digest of tools/instantiate_core.py.

Only these pins may move. Everything else is the declared closure and must
equal the base revision:
- a certificate may differ only in cli_sha256 and runtime_source_sha256, and
  a stale reference must name the base certificate;
- core-instance.json may differ only in the certificate SHA-256;
- project.zeno may differ only in that SHA-256, which it holds exactly once;
- composition.json may differ only in each instance_sha256;
- every other component file is unchanged.
Certificates come from tools/prove_core_families.py and are never written
here. tools/check_core_app.py then reproduces every component from the
catalog.
"""
from __future__ import annotations

import copy
import json
from pathlib import Path
import sys

import instantiate_core as core
import pin_producer as pins

CERTIFICATE_FIELDS = {"cli_sha256", "runtime_source_sha256"}


def applications():
    import check_core_app
    return check_core_app.app_names()


def certificate_fields(root: Path, base: str, name: str, old_pin: str | None) -> None:
    current = pins.read(root, name)
    declared = pins.at_base(root, base, name)
    if declared is None:
        raise pins.Refusal(f"{name}: certificate not declared at {base}")
    if old_pin is not None and core.digest(declared) != old_pin:
        raise pins.Refusal(f"{name}: the stale reference does not name the certificate at {base}")
    old, new = json.loads(declared), json.loads(current)
    changed = sorted(k for k in set(old) | set(new) if old.get(k) != new.get(k))
    if not set(changed) <= CERTIFICATE_FIELDS:
        raise pins.Refusal(f"{name}: change outside the declared closure (fields {changed})")


def without_reference_pin(instance: dict) -> dict:
    masked = copy.deepcopy(instance)
    masked["finite_family_certificate"]["sha256"] = None
    return masked


def component(root: Path, base: str, directory: str, changes: list, files: dict) -> bytes:
    """Rebind one component; return its planned core-instance.json bytes."""
    instance_name, project_name = directory + "/core-instance.json", directory + "/project.zeno"
    for name in pins.changed_since(root, base, [directory]):
        if name not in (instance_name, project_name):
            raise pins.Refusal(f"{name}: change outside the declared closure")
    instance, declared = pins.load(root, instance_name), pins.load_at_base(root, base, instance_name)
    reference = instance["finite_family_certificate"]
    pins.require_same(instance_name, without_reference_pin(instance), without_reference_pin(declared),
                      "a field other than the certificate SHA-256")
    project, declared_project = pins.read(root, project_name), pins.at_base(root, base, project_name)
    base_pin = declared["finite_family_certificate"]["sha256"].encode()
    if (project.count(reference["sha256"].encode()) != 1 or declared_project is None
            or project.replace(reference["sha256"].encode(), base_pin) != declared_project):
        raise pins.Refusal(f"{project_name}: change outside the declared closure (provenance comment)")
    old_pin, fresh = reference["sha256"], core.digest(pins.read(root, reference["path"]))
    certificate_fields(root, base, reference["path"], old_pin if fresh != old_pin else None)
    if fresh == old_pin:
        return pins.read(root, instance_name)
    rebound = copy.deepcopy(instance)
    rebound["finite_family_certificate"]["sha256"] = fresh
    files[instance_name] = pins.render(rebound)
    files[project_name] = project.replace(old_pin.encode(), fresh.encode())
    changes.append(pins.Change(instance_name, reference["path"], old_pin, fresh))
    changes.append(pins.Change(project_name, reference["path"], old_pin, fresh))
    return files[instance_name]


def plan(root: Path, base: str, apps=None):
    changes, files = [], {}
    for app in applications() if apps is None else apps:
        name = f"core-apps/{app}/composition.json"
        composition = pins.load(root, name)
        unpinned = lambda m: [{k: v for k, v in c.items() if k != "instance_sha256"} for c in m["components"]]
        declared = pins.load_at_base(root, base, name)
        pins.require_same(name, ({**composition, "components": None}, unpinned(composition)),
                          ({**declared, "components": None}, unpinned(declared)),
                          "a field other than an instance_sha256")
        for entry in composition["components"]:
            if entry.get("instance") != f"components/{entry['name']}/core-instance.json":
                raise pins.Refusal(f"{name}: component {entry['name']} names no core instance")
            instance = component(root, base, f"core-apps/{app}/components/{entry['name']}", changes, files)
            if core.digest(instance) != entry["instance_sha256"]:
                changes.append(pins.Change(name, f"core-apps/{app}/{entry['instance']}",
                                           entry["instance_sha256"], core.digest(instance)))
                entry["instance_sha256"] = core.digest(instance)
        rendered = pins.render(composition)
        if rendered != pins.read(root, name):
            files[name] = rendered
    return changes, files


if __name__ == "__main__":
    sys.exit(pins.main(plan, __doc__))
