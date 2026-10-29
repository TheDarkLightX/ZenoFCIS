"""Independent sixteen-case fixture from Crucible PR44, not a native receipt.

Exact guard_fixture body retained; this test helper launches no native program.
"""
TARGET = "FeedbackGuidedConstruction"


def guard_fixture(bits: tuple[bool, ...]) -> dict:
    """Independent minimal ground fixture; absence of a bit is not negation.

    Each witness bit adds both a property assertion and its filler type. Neither
    a property name nor an unrelated typed individual is sufficient by itself.
    """
    if type(bits) is not tuple or len(bits) != 4 or any(type(v) is not bool for v in bits):
        raise ValueError("four exact Boolean fixture bits required")
    facts = []
    if bits[0]:
        facts.append({"id": "process", "kind": "type", "individual": "run", "class": "Process"})
    for present, individual, prop, cls in zip(bits[1:], ("criterion", "feedback", "output"),
            ("usesCriterion", "usesFeedback", "produces"), ("Criterion", "Feedback", "ExecutableBehavior")):
        if present:
            facts.extend([
                {"id": individual + "_link", "kind": "property", "subject": "run", "property": prop, "object": individual},
                {"id": individual + "_type", "kind": "type", "individual": individual, "class": cls},
            ])
    return {
        "schema": "zal.ontology/1", "id": "classification-guard", "version": 1,
        "classes": ["Process", "Criterion", "Feedback", "ExecutableBehavior", TARGET],
        "properties": ["usesCriterion", "usesFeedback", "produces"],
        "individuals": ["run", "criterion", "feedback", "output"],
        "axioms": [{"id": "guided", "kind": "classify", "all": ["Process"],
                    "some": [{"property": "usesCriterion", "class": "Criterion"},
                             {"property": "usesFeedback", "class": "Feedback"},
                             {"property": "produces", "class": "ExecutableBehavior"}], "class": TARGET}],
        "facts": facts,
    }
