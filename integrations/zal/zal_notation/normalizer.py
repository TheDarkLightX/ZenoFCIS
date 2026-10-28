"""Cognitive compression operations with retained sources and explicit scope."""
from __future__ import annotations

import json
from copy import deepcopy
from dataclasses import dataclass
from hashlib import sha256

from .abstractions import require_formal, validate_entry
from .core import MAX_BYTES, NotationError, _bounded, digest, need


@dataclass(frozen=True)
class _NumberToken:
    # A token is a JSON number, distinct from both text and Boolean values.
    value: str


def _exact_json(source):
    need(type(source) is str, "JSON input must be text")
    need(len(source) <= MAX_BYTES, "JSON input budget exceeded")
    try:
        need(len(source.encode("utf-8")) <= MAX_BYTES, "JSON input budget exceeded")
    except UnicodeError as exc:
        raise NotationError("invalid Unicode input") from exc

    def pairs(items):
        result = {}
        for key, value in items:
            need(key not in result, f"duplicate JSON key: {key}")
            result[key] = value
        return result

    def invalid_number(value):
        raise NotationError(f"invalid number: {value}")

    try:
        value = json.loads(source, object_pairs_hook=pairs, parse_int=_NumberToken,
                           parse_float=_NumberToken, parse_constant=invalid_number)
    except (json.JSONDecodeError, RecursionError, UnicodeError, OverflowError) as exc:
        raise NotationError(f"invalid or over-budget JSON: {exc}") from exc
    need(type(value) is dict, "document must be an object")

    def check(item, depth, counter):
        if type(item) in (dict, list):
            _bounded(None, depth, counter)  # Count and bound this container.
            if type(item) is dict:
                for key, child in item.items():
                    check(key, depth + 1, counter)
                    check(child, depth + 1, counter)
            else:
                for child in item:
                    check(child, depth + 1, counter)
        else:
            # The decoder already validated each number's JSON syntax. Preserve
            # its token without converting it to a float or a bounded integer.
            _bounded(None if type(item) is _NumberToken else item, depth, counter)

    check(value, 0, [0])
    return value


def _emit_exact_json(value):
    pieces = []
    size = 0

    def append(piece):
        nonlocal size
        need(len(piece) <= MAX_BYTES - size, "serialized data budget exceeded")
        size += len(piece.encode("utf-8"))
        need(size <= MAX_BYTES, "serialized data budget exceeded")
        pieces.append(piece)

    def emit(item):
        if type(item) is _NumberToken:
            append(item.value)
        elif type(item) is dict:
            append("{")
            for index, key in enumerate(sorted(item)):
                if index:
                    append(",")
                append(json.dumps(key, ensure_ascii=False))
                append(":")
                emit(item[key])
            append("}")
        elif type(item) is list:
            append("[")
            for index, child in enumerate(item):
                if index:
                    append(",")
                emit(child)
            append("]")
        else:
            append(json.dumps(item, ensure_ascii=False, allow_nan=False))

    emit(value)
    return "".join(pieces)


def canonical_json(source):
    parsed = _exact_json(source)
    output = _emit_exact_json(parsed)
    need(_exact_json(output) == parsed, "canonical serialization changed JSON data")
    return {"operation": "canonical-json", "source": source, "result": output,
            "status": "data-equality-checked", "data_digest": sha256(output.encode("utf-8")).hexdigest(),
            "scope": "JSON structure and numeric tokens retained; keys ordered and whitespace removed; no mathematical number canonicalization",
            "authority": "none"}


def alpha_normalize(entry):
    """Return a new artifact; never replace a selected entry or its ballot history."""
    from .builtin import lambda_grammar
    from .semantics import _from_db, _to_db
    bundle = require_formal(entry)
    context = bundle["context"]
    need(context["grammar"] == lambda_grammar() and context["semantics"]["profile"] == "zal/lambda-normal-order/1",
         "alpha normalization supports only the registered closed-lambda profile")
    binding = _to_db(context["model"]["body"])
    normalized = _from_db(binding)
    need(_to_db(normalized) == binding, "normalization changed binding structure")
    result = deepcopy(entry)
    target = result["discussion"]
    target["context"]["model"]["body"] = normalized
    target["context_digest"] = digest(target["context"])
    # Paths stay fixed under alpha renaming. Prior notes/acknowledgements and
    # reasoning refer to their original context and remain in the SOURCE artifact.
    target["notes"], target["acknowledgements"], target["reasoning"] = [], [], []
    validate_entry(result)
    return {"operation": "closed-lambda-alpha", "source": deepcopy(entry), "result": result,
            "source_digest": digest(entry), "result_digest": digest(result),
            "binding_digest": digest({"de_bruijn": json.loads(json.dumps(binding))}),
            "status": "binding-structure-checked", "scope": "closed lambda alpha-equivalence; no beta evaluation or termination claim",
            "references": "term paths preserved; old discussion and evidence retained only in source",
            "adoption": "new artifact; no session or ballot mutation", "authority": "none"}
