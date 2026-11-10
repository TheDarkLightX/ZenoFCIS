"""The review desk's exchanges for a page that runs ZAL itself: a phone, a tablet or a browser extension.

    reply = portable_desk.exchange(request)        # one JSON object as text, each way

There is no server. The page's worker runs Python as WebAssembly, puts ZAL's unmodified sources beside this
file and passes text both ways. Nothing here computes a result of its own: every answer comes from
review_desk.py, as its loopback page gets it. Two things differ because the checker is in the page. No
decision table is built, since each input is explained when it is tried. And the planted edits are computed on
request, after the statement is on the screen.

This module holds the one statement the page has open. It writes no file, opens no socket and records no
agreement. Like review_desk.py it is outside the source closure that workflow.checker_identity hashes.
"""
from __future__ import annotations

import hashlib
import json
import re
import sys

import json_numbers
import review_desk
from behavior import Refusal
from workflow import checker_identity

REQUEST_BOUND = review_desk.REQUEST_BOUND   # the loopback desk's own bound on one request
LABEL = re.compile(r"[A-Za-z0-9][A-Za-z0-9 ._-]{0,79}\Z")   # the page's name for a statement; it appears in diagnostics
STAGED = {"with_edits": False, "with_table": False}


def digest_of(bundle):
    """One hash of everything the page shows about a statement, apart from the version of Python that computed it.

    A build records this for each statement it carries. The page compares, so a device whose ZAL disagrees with
    the build's says so instead of showing a result.
    """
    kept = {key: value for key, value in bundle.items() if key != "python"}
    return hashlib.sha256(json.dumps(kept, sort_keys=True, separators=(",", ":")).encode("ascii")).hexdigest()


class Desk:
    """What one page has open. The page's worker keeps one of these for as long as the page lives."""

    def __init__(self):
        self.model = self.profile = self.count = None
        self.passing = False

    def about(self):
        return {"checker": checker_identity(), "python": sys.version.split()[0], "platform": sys.platform,
                "source_bound": review_desk.MAX_SOURCE, "authority": "none"}

    def open(self, data):
        """Read one source: ZAL's results for it, or the diagnostic ZAL gives. It replaces what was open."""
        name, source = data["name"], data["source"]
        if not isinstance(name, str) or not LABEL.match(name) or not isinstance(source, str):
            raise Refusal("A statement needs a short plain name and its source text")
        self.model = self.profile = self.count = None
        self.passing = False
        # Encoded as it is, so text that is not ASCII reaches ZAL's own rule about sources and is refused there.
        bundle, model, profile, error = review_desk.read_source(source.encode("utf-8", "surrogatepass"), name, **STAGED)
        if error is not None:
            return {"error": error}
        self.model, self.profile = model, profile
        self.count = bundle["check"].get("input_tuples") or sum(1 for _ in review_desk.input_tuples(model, profile))
        self.passing = bundle["check"]["status"] in review_desk.PASSING
        return {"bundle": bundle, "digest": digest_of(bundle), "edits_follow": self.passing}

    def current(self, data):
        if self.model is None:
            raise Refusal("ZAL does not accept the open text; correct it and check again")
        if data.get("revision") != self.model.revision:
            raise Refusal("This page shows an older revision; reload")

    def edits(self, data):
        """The desk's planted rule edits for the open statement, as the loopback page shows them."""
        self.current(data)
        if not self.passing:
            return None
        return review_desk.planted_edits(self.model, self.profile, self.count, checker_identity())

    def fires(self, data):
        """The first declared input, in ZAL's own order, that the named rule decides; None when no input does."""
        self.current(data)
        if data.get("rule") not in {rule.id for rule in self.model.rules}:
            raise Refusal("The open statement has no rule of that name")
        for step in review_desk.decided_steps(self.model, self.profile):
            if step.rule == data["rule"]:
                return {"state": step.state, "variables": dict(step.values), "event": step.event, "context": dict(step.context)}
        return None

    def answer(self, data):
        route = data.get("route")
        if route == "about":
            return self.about()
        if route == "open":
            return self.open(data)
        if route == "edits":
            return self.edits(data)
        if route == "fires":
            return self.fires(data)
        self.current(data)
        return review_desk.ask(self.model, self.profile, route, data)

    def exchange(self, text):
        """One request in, one reply out, both JSON text. It never raises: a refusal or an error is a reply."""
        try:
            if not isinstance(text, str) or not 0 < len(text) <= REQUEST_BOUND:
                raise Refusal("Request is empty or exceeds the review desk bound")
            data = json.loads(text, **json_numbers.hooks(json_numbers.raising(Refusal, "request: ")))
            if not isinstance(data, dict):
                raise Refusal("Expected a review desk request object")
            reply = {"ok": True, "reply": self.answer(data)}
        except (ValueError, KeyError, TypeError) as error:            # ZAL's refusal or a malformed request, as the loopback desk answers
            reply = {"ok": False, "error": str(error)[:2000] or type(error).__name__}
        except Exception as error:                                     # not a refusal by ZAL: the page says so
            reply = {"ok": False, "error": f"{type(error).__name__}: {error}"[:2000], "unexpected": True}
        return json.dumps(reply, separators=(",", ":"))


exchange = Desk().exchange
