"""Recover source and replay generated SVG; this is not optical parsing."""
from __future__ import annotations

import re
import xml.etree.ElementTree as ET

from .core import (
    MAX_BYTES,
    NotationError,
    digest,
    keys,
    loads,
    model_digest,
    need,
    validate_model,
    validate_pack,
)
from .render import RENDERER, render_svg


def recover_source(svg: str) -> dict:
    try:
        need(type(svg) is str and len(svg.encode("utf-8")) <= MAX_BYTES, "SVG input budget exceeded")
        need(re.search(r"<!\s*(?:DOCTYPE|ENTITY)", svg, re.IGNORECASE) is None, "DTD/entity declarations are unsupported")
        root = ET.fromstring(svg)
        need(root.tag == "{http://www.w3.org/2000/svg}svg", "expected SVG namespace/root")
        metadata = [e for e in root.findall("{http://www.w3.org/2000/svg}metadata") if e.get("id") == "zal-source"]
        need(len(metadata) == 1 and not list(metadata[0]), "expected exactly one text-only ZAL source metadata element")
        source = loads(metadata[0].text or "")
        keys(source, {"schema", "grammar", "model", "notation", "view", "model_digest", "notation_digest", "renderer"}, where="SVG source")
        need(source["schema"] == "zal.diagram-source/1" and source["renderer"] == RENDERER, "unsupported SVG source/renderer version")
        keys(source["view"], {"folded"}, where="SVG view")
        need(type(source["view"]["folded"]) is list and len(source["view"]["folded"]) <= 512 and all(type(p) is str for p in source["view"]["folded"]), "invalid SVG folded view")
        validate_model(source["grammar"], source["model"])
        validate_pack(source["grammar"], source["notation"])
        need(source["model_digest"] == model_digest(source["grammar"], source["model"]), "SVG model identity mismatch")
        need(source["notation_digest"] == digest(source["notation"]), "SVG notation identity mismatch")
        return source
    except (ET.ParseError, UnicodeError, RecursionError) as exc:
        raise NotationError(f"malformed SVG source: {exc}") from exc


def audit_svg(svg: str) -> dict:
    source = recover_source(svg)
    expected = render_svg(source["grammar"], source["model"], source["notation"], folded=tuple(source["view"]["folded"]))
    return {"schema": "zal.svg-replay/1", "status": "matches-current-renderer" if svg == expected else "rendering-drift",
            "model_digest": source["model_digest"], "notation_digest": source["notation_digest"],
            "scope": "exact serialized SVG replay with this renderer; no optical parsing or semantic theorem",
            "authority": "none"}
