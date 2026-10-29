"""Bounded named-fact reasoning for an explicitly admitted OWL 2 RL fragment.

Only positive subclass/classification, subproperty, domain and range rules are
materialized. Existential restrictions occur in classification antecedents;
they inspect existing links and never construct individuals. Class disjointness
detects a contradiction with a witness, without applying logical explosion.

This is not a general OWL, SWRL, RDF or closed-world validator. Completion means
the named ground type/property consequences of this fragment were saturated.
An absent fact is not asserted false. Source metadata is inert, and inferred
types never promote an abstraction, receipt or publication status. All mutable
indexes are exclusively owned by one invocation; outputs retain no input alias.
"""
from __future__ import annotations

from collections import defaultdict, deque
import copy

from .core import NotationError, canonical, digest, keys, name, need


SCHEMA = "zal.ontology/1"
PROFILE = "zal.ontology/positive-rl/1"
CHECKER = "zal.ontology-closure/1"
MAX_CLASSES = 64
MAX_PROPERTIES = 32
MAX_INDIVIDUALS = 128
MAX_AXIOMS = 128
MAX_FACTS = 1024
MAX_WORK = 1_000_000
MAX_GUARDS = 8
AXIOM_KINDS = frozenset({"subclass", "classify", "subproperty", "domain", "range", "disjoint"})


class UnsupportedOntology(NotationError):
    """A construct has no semantics in this profile; admission is all-or-nothing."""

    def __init__(self, reason: str, detail: str):
        super().__init__(detail)
        self.reason = reason


def _names(values, maximum, where):
    need(type(values) is list and len(values) <= maximum, f"{where}: expected at most {maximum} names")
    for value in values:
        name(value, where)
    need(len(set(values)) == len(values), f"{where}: duplicate name")
    return set(values)


def _reference(value, declared, where):
    name(value, where)
    need(value in declared, f"{where}: undeclared symbol {value!r}")


def _fact_key(fact):
    if fact["kind"] == "type":
        return ("type", fact["individual"], fact["class"])
    return ("property", fact["subject"], fact["property"], fact["object"])


def _fact_body(key):
    if key[0] == "type":
        return {"kind": "type", "individual": key[1], "class": key[2]}
    return {"kind": "property", "subject": key[1], "property": key[2], "object": key[3]}


def validate_ontology(document: dict) -> None:
    """Admit the complete inert document before copying or reasoning.

    Malformed data raises NotationError. Unsupported schema/axiom kinds raise
    its UnsupportedOntology subclass. No supported prefix is interpreted when
    any unsupported construct is present. Names are opaque symbols, never
    implicitly OWL vocabulary or dereferenced IRIs. This subset has no punning.
    """
    canonical(document)
    need(type(document) is dict, "ontology must be an object")
    need(type(document.get("schema")) is str, "ontology schema must be text")
    if document["schema"] != SCHEMA:
        raise UnsupportedOntology("unsupported-schema", "unsupported ontology schema")
    keys(document, {"schema", "id", "version", "classes", "properties", "individuals", "axioms", "facts"},
         {"description", "origin"}, "ontology")
    name(document["id"], "ontology id")
    need(type(document["version"]) is int and 1 <= document["version"] <= 1_000_000,
         "ontology version must be an integer in 1..1000000")
    if "description" in document:
        need(type(document["description"]) is str, "ontology description must be text")
    classes = _names(document["classes"], MAX_CLASSES, "classes")
    properties = _names(document["properties"], MAX_PROPERTIES, "properties")
    individuals = _names(document["individuals"], MAX_INDIVIDUALS, "individuals")
    need(not (classes & properties or classes & individuals or properties & individuals),
         "declaration namespaces must be disjoint; punning is unsupported")
    need(type(document["axioms"]) is list and len(document["axioms"]) <= MAX_AXIOMS,
         "ontology axiom budget exceeded")
    need(type(document["facts"]) is list and len(document["facts"]) <= MAX_FACTS,
         "ontology assertion budget exceeded")
    identifiers = set()
    for axiom in document["axioms"]:
        need(type(axiom) is dict, "axiom must be an object")
        name(axiom.get("id"), "axiom id")
        need(axiom["id"] not in identifiers, "duplicate assertion or axiom id")
        identifiers.add(axiom["id"])
        need(type(axiom.get("kind")) is str, "axiom kind must be text")
        kind = axiom["kind"]
        if kind not in AXIOM_KINDS:
            # Input strings may already consume the entire exchange string
            # budget. Do not enlarge them by echoing repr(kind) in a report.
            raise UnsupportedOntology("unsupported-axiom-kind", "unsupported axiom kind")
        if kind in {"subclass", "subproperty"}:
            keys(axiom, {"id", "kind", "sub", "super"}, where="inclusion axiom")
            declared = classes if kind == "subclass" else properties
            _reference(axiom["sub"], declared, "inclusion sub")
            _reference(axiom["super"], declared, "inclusion super")
        elif kind in {"domain", "range"}:
            keys(axiom, {"id", "kind", "property", "class"}, where="property typing axiom")
            _reference(axiom["property"], properties, "typing property")
            _reference(axiom["class"], classes, "typing class")
        elif kind == "disjoint":
            keys(axiom, {"id", "kind", "classes"}, where="disjointness axiom")
            _names(axiom["classes"], 2, "disjoint classes")
            need(len(axiom["classes"]) == 2, "disjointness requires two distinct classes")
            for selected in axiom["classes"]:
                _reference(selected, classes, "disjoint class")
        else:
            keys(axiom, {"id", "kind", "all", "some", "class"}, where="classification axiom")
            _names(axiom["all"], MAX_GUARDS, "classification all guards")
            need(type(axiom["some"]) is list and len(axiom["some"]) <= MAX_GUARDS,
                 "classification existential guard budget exceeded")
            need(1 <= len(axiom["all"]) + len(axiom["some"]) <= MAX_GUARDS,
                 "classification needs 1..8 antecedent guards")
            for selected in axiom["all"]:
                _reference(selected, classes, "classification antecedent class")
            seen = set()
            for guard in axiom["some"]:
                keys(guard, {"property", "class"}, where="existential antecedent")
                _reference(guard["property"], properties, "existential property")
                _reference(guard["class"], classes, "existential filler class")
                pair = (guard["property"], guard["class"])
                need(pair not in seen, "duplicate existential antecedent")
                seen.add(pair)
            _reference(axiom["class"], classes, "classification head")
    asserted = set()
    for fact in document["facts"]:
        need(type(fact) is dict, "assertion must be an object")
        name(fact.get("id"), "assertion id")
        need(fact["id"] not in identifiers, "duplicate assertion or axiom id")
        identifiers.add(fact["id"])
        need(type(fact.get("kind")) is str, "assertion kind must be text")
        if fact["kind"] == "type":
            keys(fact, {"id", "kind", "individual", "class"}, where="type assertion")
            _reference(fact["individual"], individuals, "typed individual")
            _reference(fact["class"], classes, "asserted class")
        elif fact["kind"] == "property":
            keys(fact, {"id", "kind", "subject", "property", "object"}, where="object-property assertion")
            _reference(fact["subject"], individuals, "property subject")
            _reference(fact["object"], individuals, "property object")
            _reference(fact["property"], properties, "asserted property")
        else:
            raise UnsupportedOntology("unsupported-assertion-kind", "unsupported assertion kind")
        key = _fact_key(fact)
        need(key not in asserted, "duplicate asserted fact")
        asserted.add(key)


class _Stop(Exception):
    def __init__(self, reason, contradiction=None):
        self.reason = reason
        self.contradiction = contradiction


class _Budget:
    def __init__(self, maximum):
        self.maximum = maximum
        self.work = 0

    def spend(self):
        if self.work == self.maximum:
            raise _Stop("work-budget")
        self.work += 1


def _limits(max_facts, max_work):
    need(type(max_facts) is int and 0 <= max_facts <= MAX_FACTS,
         "max_facts must be an integer in 0..1024")
    need(type(max_work) is int and 0 <= max_work <= MAX_WORK,
         "max_work must be an integer in 0..1000000")
    return {"max_facts": max_facts, "max_work": max_work}


def _base(document, limits):
    identity = {"ontology_digest": digest(document), "profile": PROFILE, "checker": CHECKER}
    return {"schema": "zal.ontology-classification/1", "profile": PROFILE, "checker": CHECKER,
            "ontology_digest": identity["ontology_digest"], "source_identity": identity,
            "status": "unsupported", "complete": False, "consistency": "unknown",
            "facts": [], "proofs": [], "types": {}, "limits": limits,
            "usage": {"work": 0, "facts": 0, "processed_facts": 0},
            "absence": "not-derived is unknown, never a negative class or property assertion",
            "scope": "Named ground type/property closure of the admitted positive RL fragment; disjointness contradictions only; no full OWL entailment, code equivalence, receipt validation or authority.",
            "authority": "none"}


def classify_ontology(document: dict, *, max_facts: int = MAX_FACTS, max_work: int = MAX_WORK) -> dict:
    """Return deterministic closure and a first acyclic derivation per fact.

    All premise scans, candidate classification guards, reverse-link scans and
    fact insertion attempts are charged. Validation is separately bounded by
    the admitted JSON/collection limits. There is no unmetered Cartesian join.
    Unsupported input yields no conclusions. Exhaustion retains only already
    justified facts, marks completeness false, and never asserts consistency.
    """
    limits = _limits(max_facts, max_work)
    try:
        validate_ontology(document)
    except UnsupportedOntology as error:
        report = _base(document, limits)
        report.update(reason=error.reason, detail=str(error))
        return report
    source = copy.deepcopy(document)
    report = _base(source, limits)
    budget = _Budget(max_work)
    known, proofs = {}, {}
    types = defaultdict(set)
    links = defaultdict(set)
    incoming = defaultdict(set)
    agenda = deque()
    inclusions, property_rules, disjoint = defaultdict(list), defaultdict(list), defaultdict(list)
    classifications = []
    processed = 0

    def add(key, rule, axiom=None, premises=(), assertion=None):
        budget.spend()
        if key in known:
            return
        if len(known) == max_facts:
            raise _Stop("fact-budget")
        body = _fact_body(key)
        fact_id = "f-" + digest(body)
        known[key] = {"id": fact_id, **body}
        # Every premise already exists before insertion; a stored proof cannot
        # refer to itself or a later fact, including in cyclic subclass inputs.
        proofs[key] = {"fact": fact_id, "rule": rule, "axiom": axiom,
                       "premises": sorted({known[p]["id"] for p in premises}), "assertion": assertion}
        if key[0] == "type":
            types[key[1]].add(key[2])
        else:
            links[(key[1], key[2])].add(key[3])
            incoming[key[3]].add((key[1], key[2]))
        agenda.append(key)
        if key[0] == "type":
            for other, axiom_id in disjoint[key[2]]:
                budget.spend()
                opposite = ("type", key[1], other)
                if opposite in known:
                    raise _Stop("disjoint-types", {"individual": key[1], "axiom": axiom_id,
                        "classes": sorted([key[2], other]),
                        "premises": sorted([fact_id, known[opposite]["id"]])})

    def classify(individual):
        for axiom in classifications:
            budget.spend()
            head = ("type", individual, axiom["class"])
            if head in known:
                continue
            premises = []
            matched = True
            for class_name in sorted(axiom["all"]):
                budget.spend()
                premise = ("type", individual, class_name)
                if premise not in known:
                    matched = False
                    break
                premises.append(premise)
            if not matched:
                continue
            for guard in sorted(axiom["some"], key=lambda g: (g["property"], g["class"])):
                budget.spend()
                witness = None
                for target in sorted(links.get((individual, guard["property"]), ())):
                    budget.spend()
                    if guard["class"] in types.get(target, ()):
                        witness = target
                        break
                if witness is None:
                    matched = False
                    break
                premises.extend([("property", individual, guard["property"], witness),
                                 ("type", witness, guard["class"])])
            if matched:
                add(head, "classify", axiom["id"], premises)

    try:
        for axiom in sorted(source["axioms"], key=lambda a: a["id"]):
            budget.spend()
            kind = axiom["kind"]
            if kind == "subclass":
                inclusions[axiom["sub"]].append(axiom)
            elif kind in {"subproperty", "domain", "range"}:
                property_rules[axiom["sub"] if kind == "subproperty" else axiom["property"]].append(axiom)
            elif kind == "disjoint":
                a, b = axiom["classes"]
                disjoint[a].append((b, axiom["id"]))
                disjoint[b].append((a, axiom["id"]))
            else:
                classifications.append(axiom)
        for fact in sorted(source["facts"], key=_fact_key):
            add(_fact_key(fact), "asserted", assertion=fact["id"])
        while agenda:
            budget.spend()
            key = agenda.popleft()
            processed += 1
            if key[0] == "type":
                for axiom in inclusions[key[2]]:
                    budget.spend()
                    add(("type", key[1], axiom["super"]), "subclass", axiom["id"], [key])
                classify(key[1])
                for subject, _ in sorted(incoming.get(key[1], ())):
                    budget.spend()
                    classify(subject)
            else:
                for axiom in property_rules[key[2]]:
                    budget.spend()
                    kind = axiom["kind"]
                    head = (("property", key[1], axiom["super"], key[3]) if kind == "subproperty"
                            else ("type", key[1] if kind == "domain" else key[3], axiom["class"]))
                    add(head, kind, axiom["id"], [key])
                classify(key[1])
        report.update(status="classified", complete=True, consistency="consistent")
    except _Stop as stop:
        if stop.contradiction is not None:
            report.update(status="inconsistent", consistency="inconsistent", reason=stop.reason,
                          contradiction=stop.contradiction)
        else:
            report.update(status="incomplete", reason=stop.reason)
    ordered = sorted(known)
    report.update(facts=[known[key] for key in ordered], proofs=[proofs[key] for key in ordered],
                  types={individual: sorted(types.get(individual, ())) for individual in sorted(source["individuals"])},
                  usage={"work": budget.work, "facts": len(known), "processed_facts": processed})
    try:
        canonical(report)
    except NotationError:
        # The exchange budget also bounds unusually long names/proof lists.
        # Refuse the report rather than silently dropping proof premises.
        report = _base(source, limits)
        report.update(status="incomplete", reason="report-budget",
                      usage={"work": budget.work, "facts": len(known), "processed_facts": processed})
        canonical(report)
    return report


def replay_ontology(document: dict, report: dict) -> dict:
    """Recompute the entire result with its exact limits; imported labels are inert.

    A matching incomplete, inconsistent or unsupported result is not promoted
    to a successful classification. Malformed replay requests raise NotationError.
    """
    canonical(report)
    need(type(report) is dict, "ontology replay report must be an object")
    limits = report.get("limits")
    keys(limits, {"max_facts", "max_work"}, where="ontology replay limits")
    actual = classify_ontology(document, **limits)
    matched = canonical(report) == canonical(actual)
    return {"schema": "zal.ontology-replay/1", "status": "replayed" if matched else "mismatch",
            "matches_replay": matched, "classification_status": actual["status"],
            "complete": actual["complete"], "ontology_digest": actual["ontology_digest"],
            "source_identity": copy.deepcopy(actual["source_identity"]), "authority": "none"}


def ontology_example() -> dict:
    """Illustrative asserted structure, not evidence that the processes are equivalent."""
    classes = ["Process", "TrainingProcess", "ProgrammingProcess", "AlignmentAnalysis", "RequirementsAnalysis",
               "Criterion", "Feedback", "ExecutableBehavior", "FeedbackGuidedConstruction", "Intent",
               "Assessment", "IntentCriterionReview"]
    properties = ["usesCriterion", "usesFeedback", "produces", "usesIntent"]
    individuals = ["trainingRun", "programmingSession", "alignmentReview", "requirementsReview", "lossObjective",
                   "requirementsCriterion", "gradientFeedback", "testResults", "learnedModel", "executable",
                   "stakeholderIntent", "alignmentCriterion", "alignmentAssessment", "requirementsAssessment"]
    axioms = [{"id": "process-" + cls, "kind": "subclass", "sub": cls, "super": "Process"}
              for cls in ["TrainingProcess", "ProgrammingProcess", "AlignmentAnalysis", "RequirementsAnalysis"]]
    axioms.extend([
        {"id": "feedback-construction", "kind": "classify", "all": ["Process"],
         "some": [{"property": "usesCriterion", "class": "Criterion"},
                  {"property": "usesFeedback", "class": "Feedback"},
                  {"property": "produces", "class": "ExecutableBehavior"}], "class": "FeedbackGuidedConstruction"},
        {"id": "intent-criterion-review", "kind": "classify", "all": ["Process"],
         "some": [{"property": "usesIntent", "class": "Intent"},
                  {"property": "usesCriterion", "class": "Criterion"},
                  {"property": "produces", "class": "Assessment"}], "class": "IntentCriterionReview"}])
    facts = []
    def typed(individual, class_name):
        facts.append({"id": f"assertion-{len(facts) + 1}", "kind": "type", "individual": individual, "class": class_name})
    def linked(subject, prop, obj):
        facts.append({"id": f"assertion-{len(facts) + 1}", "kind": "property", "subject": subject, "property": prop, "object": obj})
    for individual, cls in [("trainingRun", "TrainingProcess"), ("programmingSession", "ProgrammingProcess"),
                            ("alignmentReview", "AlignmentAnalysis"), ("requirementsReview", "RequirementsAnalysis"),
                            ("lossObjective", "Criterion"), ("requirementsCriterion", "Criterion"),
                            ("alignmentCriterion", "Criterion"), ("gradientFeedback", "Feedback"), ("testResults", "Feedback"),
                            ("learnedModel", "ExecutableBehavior"), ("executable", "ExecutableBehavior"),
                            ("stakeholderIntent", "Intent"), ("alignmentAssessment", "Assessment"),
                            ("requirementsAssessment", "Assessment")]:
        typed(individual, cls)
    for subject, criterion, feedback, output in [("trainingRun", "lossObjective", "gradientFeedback", "learnedModel"),
                                                ("programmingSession", "requirementsCriterion", "testResults", "executable")]:
        linked(subject, "usesCriterion", criterion)
        linked(subject, "usesFeedback", feedback)
        linked(subject, "produces", output)
    for subject, criterion, output in [("alignmentReview", "alignmentCriterion", "alignmentAssessment"),
                                      ("requirementsReview", "requirementsCriterion", "requirementsAssessment")]:
        linked(subject, "usesIntent", "stakeholderIntent")
        linked(subject, "usesCriterion", criterion)
        linked(subject, "produces", output)
    document = {"schema": SCHEMA, "id": "zal/examples/shared-structure", "version": 1,
                "classes": classes, "properties": properties, "individuals": individuals,
                "axioms": axioms, "facts": facts,
                "description": "Shared structural categories for training/programming and alignment/requirements review.",
                "origin": {"kind": "synthetic-example", "assumptions": "Relationships and filler types are illustrative assertions.",
                           "scope": "Membership in a common class establishes neither process equivalence nor requirement completeness."}}
    validate_ontology(document)
    return document
