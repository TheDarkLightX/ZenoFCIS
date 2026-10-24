import CollectionsPatterns

/-! Executable unordered-view specification. The differential supplies only
well-typed bounded shapes; structural admission is independent of law syntax.
An identical state-law truth value is conjoined after this predicate. -/
namespace CollectionsPatternsExecutable
open CollectionsPatterns

structure Domain where
  least : Nat
  greatest : Nat
  deriving Repr, DecidableEq

structure KeyPosition where
  position : Nat
  keyType : Nat
  deriving Repr, DecidableEq

structure Entry where
  mapId : Nat
  key : Nat
  presence : Nat
  fields : List Nat
  storedKeys : List KeyPosition
  deriving Repr, DecidableEq

structure Summary where
  mapId : Nat
  position : Nat
  field : Option Nat
  deriving Repr, DecidableEq

structure Tally where
  target : Nat
  field : Nat
  source : Nat
  group : Nat
  weight : Option Nat
  selector : Option Nat
  deriving Repr, DecidableEq

structure Shape where
  domains : List Domain
  keys : List KeyPosition
  entries : List Entry
  summaries : List Summary
  tallies : List Tally
  deriving Repr, DecidableEq

abbrev Tuple := List Nat
def valueAt (tuple : Tuple) (position : Nat) := tuple.getD position 0
def present (tuple : Tuple) (entry : Entry) := valueAt tuple entry.presence == 1
def fieldAt (tuple : Tuple) (entry : Entry) (field : Nat) := valueAt tuple (entry.fields.getD field 0)

/-- Exactly the canonical image of C4's first-occurrence numbering. -/
def canonicalClasses (labels : List Nat) : Bool := decide (labels = normalize labels)

theorem canonical_classes_iff (labels : List Nat) :
    canonicalClasses labels = true ↔ Grows 0 labels := by
  constructor
  · intro h
    have he : labels = normalize labels := of_decide_eq_true h
    rw [he]
    exact encode_grows [] labels
  · intro h
    exact decide_eq_true (normalize_fixed labels h).symm

def activeKeyAt (shape : Shape) (tuple : Tuple) (position : Nat) : Option KeyPosition :=
  match shape.keys.find? (fun key => key.position == position) with
  | some key => some key
  | none => (shape.entries.filter (present tuple)).flatMap Entry.storedKeys |>.find?
      (fun key => key.position == position)

def activeKeys (shape : Shape) (tuple : Tuple) : List KeyPosition :=
  (List.range shape.domains.length).filterMap (activeKeyAt shape tuple)

def keyClasses (shape : Shape) (tuple : Tuple) : Bool :=
  let active := activeKeys shape tuple
  let types := (active.map KeyPosition.keyType).eraseDups
  types.all (fun keyType => canonicalClasses
    ((active.filter (fun key => key.keyType == keyType)).map (fun key => valueAt tuple key.position)))

def canonicalAbsence (shape : Shape) (tuple : Tuple) : Bool :=
  shape.entries.all (fun entry => present tuple entry || entry.fields.all (fun field =>
    valueAt tuple field == (shape.domains.getD field ⟨0, 0⟩).least))

def aliasesAgree (shape : Shape) (tuple : Tuple) : Bool :=
  shape.entries.all (fun left => shape.entries.all (fun right =>
    !(left.mapId == right.mapId && valueAt tuple left.key == valueAt tuple right.key) ||
      (valueAt tuple left.presence == valueAt tuple right.presence &&
        (left.fields.map (valueAt tuple)) == (right.fields.map (valueAt tuple)))))

def representatives (shape : Shape) (tuple : Tuple) (mapId : Nat) : List Entry :=
  (shape.entries.filter (fun entry => entry.mapId == mapId)).foldl
    (fun selected entry =>
      if selected.any (fun earlier => valueAt tuple earlier.key == valueAt tuple entry.key)
      then selected else selected ++ [entry]) []

def amount (tuple : Tuple) (entry : Entry) (field : Option Nat) : Nat :=
  if present tuple entry then field.elim 1 (fieldAt tuple entry) else 0

def namedSum (shape : Shape) (tuple : Tuple) (summary : Summary) : Nat :=
  ((representatives shape tuple summary.mapId).map (fun entry => amount tuple entry summary.field)).sum

def summariesCover (shape : Shape) (tuple : Tuple) : Bool :=
  shape.summaries.all (fun summary => decide (namedSum shape tuple summary ≤ valueAt tuple summary.position))

def grouped (shape : Shape) (tuple : Tuple) (tally : Tally) (key : Nat) : Nat :=
  ((representatives shape tuple tally.source).map (fun entry =>
    if present tuple entry && tally.selector.elim true (fun j => fieldAt tuple entry j == 1) &&
        fieldAt tuple entry tally.group == key
    then tally.weight.elim 1 (fieldAt tuple entry) else 0)).sum

def talliesCover (shape : Shape) (tuple : Tuple) : Bool :=
  shape.tallies.all (fun tally => (representatives shape tuple tally.target).all (fun entry =>
    decide (grouped shape tuple tally (valueAt tuple entry.key) ≤ amount tuple entry (some tally.field))))

def structuralAdmissible (shape : Shape) (tuple : Tuple) : Bool :=
  canonicalAbsence shape tuple && keyClasses shape tuple && aliasesAgree shape tuple &&
    summariesCover shape tuple && talliesCover shape tuple

def admissible (shape : Shape) (tuple : Tuple) (stateLawsHold : Bool) : Bool :=
  structuralAdmissible shape tuple && stateLawsHold

/-- Equality of structural admission lifts to either truth value of the state-law
filter. The differential does not claim to verify the formula interpreter. -/
theorem law_filter_ext (left right : Bool) (h : left = right) (stateLawsHold : Bool) :
    (left && stateLawsHold) = (right && stateLawsHold) := by rw [h]

def tuples : List Domain → List Tuple
  | [] => [[]]
  | domain :: rest =>
    (List.range (domain.greatest + 1 - domain.least)).flatMap (fun offset =>
      (tuples rest).map (List.cons (domain.least + offset)))

def admitted (shape : Shape) : List Tuple :=
  (tuples shape.domains).filter (structuralAdmissible shape)

#print axioms canonical_classes_iff
#print axioms law_filter_ext
end CollectionsPatternsExecutable
