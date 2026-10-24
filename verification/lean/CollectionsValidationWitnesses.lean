import CollectionsPatterns
import CollectionsReductionValidation
import CollectionsSignedTallies

/-! Premise boundaries and executable rejection witnesses. These are specification
checks; parser, evaluator and storage correspondence remain separate obligations. -/
namespace CollectionsValidationWitnesses
open CollectionsAggregate CollectionsReduction CollectionsReductionGenesis
open CollectionsReductionTallies CollectionsReductionValidation

/-- Once P2 factors actual decisions through the equality-only view, P1's
renaming consequence follows. It is not an independent induction premise. -/
theorem p1_follows_from_p2 {maps sums : Nat} {K L R : Fin maps → Type} {S I : Type}
    [∀ m, DecidableEq (K m)] [∀ m, DecidableEq (L m)]
    (arity : Fin maps → Nat) (core : View R S I sums arity → Update R S sums arity)
    (actualK : State K R S sums → Binding K arity → I → Update R S sums arity)
    (actualL : State L R S sums → Binding L arity → I → Update R S sums arity)
    (p2K : ∀ s b input, actualK s b input = core (frame s b input))
    (p2L : ∀ s b input, actualL s b input = core (frame s b input))
    (s : State K R S sums) (t : State L R S sums)
    (a : Binding K arity) (b : Binding L arity) (input : I)
    (hs : s.scalar = t.scalar) (ht : s.summary = t.summary)
    (he : ∀ m i, lookup (a.key m i) (s.rows m) = lookup (b.key m i) (t.rows m))
    (hp : ∀ m i j, a.key m i = a.key m j ↔ b.key m i = b.key m j) :
    actualK s a input = actualL t b input := by
  rw [p2K, p2L]
  exact decision_renaming core s t a b input hs ht he hp

def hiddenRead (rows : List (Bool × Bool)) (extra : Bool) : Option Bool :=
  if (lookup extra rows).isSome then some true else none

/-- P2: the two named views agree and both stored states satisfy the entry law,
but an unnamed read changes the actual decision and commits a forbidden value. -/
theorem p2_hidden_read_counterexample :
    lookup false ([] : List (Bool × Bool)) = lookup false [(true, false)] ∧
    (∀ k : Bool, lookup k [(true, false)] ≠ some true) ∧
    hiddenRead [] true = none ∧ hiddenRead [(true, false)] true = some true ∧
    lookup false (put false (hiddenRead [(true, false)] true) [(true, false)]) = some true := by
  unfold hiddenRead
  decide

/-- P3: the preserved left-role law is false when the written right key later
appears under the left role. The imported witness uses no classical split. -/
theorem p3_role_counterexample :
    (∀ a : Bool, Collections.zeroEntry ((fun _ : Bool => none) a)) ∧
    Collections.zeroEntry (Collections.roleStep (fun _ : Bool => none) false true false) ∧
    ¬ Collections.zeroEntry (Collections.roleStep (fun _ : Bool => none) false true true) :=
  Collections.role_only_counterexample

/-- P4: the checked named write is harmless; an extra unnamed write is not. -/
theorem p4_unnamed_write_counterexample :
    (∀ k : Bool, lookup k (applyWrites ([] : List (Bool × Bool)) [(false, none)]) ≠ some true) ∧
    lookup false (applyWrites [] [(false, none), (true, some true)]) = none ∧
    lookup true (applyWrites [] [(false, none), (true, some true)]) = some true := by decide

/-- T1: changing the interpretation of the stored grouping field changes the
invariant without any record update, so a no-write delta check cannot cover it. -/
theorem t1_fixed_group_counterexample :
    Invariant (fun n : Nat => n) (fun r : Bool => r) (fun _ => 1) (fun _ => true)
      [(false, 1)] [(false, false)] ∧
    DeltaChecked (fun n : Nat => n) (fun r : Bool => r) (fun _ => 1) (fun _ => true)
      [(false, 1)] [(false, false)] [] [] ∧
    ¬ Invariant (fun n : Nat => n) (fun r : Bool => !r) (fun _ => 1) (fun _ => true)
      [(false, 1)] [(false, false)] := by
  unfold Invariant DeltaChecked
  decide

/-- T2 is not needed by signed conservation, proved in CollectionsSignedTallies.
It is needed for the checker coverage inequality: a selected positive entry can
exceed a zero whole aggregate when an unnamed contribution is negative. -/
theorem t2_negative_coverage_counterexample :
    CollectionsSignedTallies.total (fun x : Int => x) [(false, 1), (true, -1)] = 0 ∧
    ¬ CollectionsSignedTallies.namedTotal (fun x : Int => x)
      [(false, 1), (true, -1)] [(false, none)] ≤
        CollectionsSignedTallies.total (fun x : Int => x) [(false, 1), (true, -1)] := by decide

/-- T3: a nonempty inconsistent initial source remains inconsistent through
perfectly matching zero deltas. Empty genesis cannot be omitted. -/
theorem t3_nonempty_genesis_counterexample :
    DeltaChecked (fun n : Nat => n) (fun _ : Unit => false) (fun _ => 1) (fun _ => true)
      [] [(false, ())] [] [] ∧
    ¬ Invariant (fun n : Nat => n) (fun _ : Unit => false) (fun _ => 1) (fun _ => true)
      [] [(false, ())] := by
  unfold DeltaChecked Invariant
  decide

/-- T4: adding an on-loan copy without its member increment violates the tally. -/
theorem t4_missing_target_delta_counterexample :
    Invariant (fun n : Nat => n) (fun _ : Unit => false) (fun _ => 1) (fun _ => true)
      ([] : List (Bool × Nat)) ([] : List (Bool × Unit)) ∧
    ¬ DeltaChecked (fun n : Nat => n) (fun _ : Unit => false) (fun _ => 1) (fun _ => true)
      [] [] [(false, none)] [(false, some ())] ∧
    ¬ Invariant (fun n : Nat => n) (fun _ : Unit => false) (fun _ => 1) (fun _ => true)
      (applyWrites [] [(false, none)]) (applyWrites [] [(false, some ())]) := by
  unfold Invariant DeltaChecked
  decide

def canonicalRaw (least : Nat) (raw : RecordView Nat) : RecordView Nat :=
  if raw.present then raw else ⟨false, least⟩

/-- Changed aliases must agree before the selected stored successor is made canonical. -/
def mergeGroup (least : Nat) (before : RecordView Nat) (candidates : List (RecordView Nat)) :
    Option (RecordView Nat) :=
  match candidates.filter (fun r => decide (r ≠ before)) with
  | [] => some (canonicalRaw least before)
  | changed :: rest =>
    if rest.all (fun r => decide (r = changed)) then some (canonicalRaw least changed) else none

def mergedLawful (least : Nat) (before : RecordView Nat) (candidates : List (RecordView Nat))
    (law : RecordView Nat → Bool) : Bool :=
  (mergeGroup least before candidates).any law

def summaryDelta (before after oldNamed newNamed : Nat) : Bool :=
  decide (after + oldNamed = before + newNamed)

def genesisAccepted (least : Nat) (entry : RecordView Nat) (summary : Nat)
    (laws : List (RecordView Nat → Bool)) : Bool :=
  !entry.present && decide (entry.fields = least) && decide (summary = 0) && laws.all (fun law => law entry)

def boundPostKey (bindings : List Nat) (post : Option Nat) : Bool :=
  match post with
  | none => true
  | some key => bindings.contains key

inductive Verdict where
  | checked | refuted | unknown
  deriving DecidableEq

/-- The collection-judge outcome distinguishes falsity, undefinedness and vacuity. -/
def transitionVerdict (committed failed undefined : Nat) : Verdict :=
  if failed > 0 then .refuted
  else if undefined > 0 || committed = 0 then .unknown
  else .checked

def signedFits (n : Int) : Bool := decide (-(2 ^ 127) ≤ n ∧ n < 2 ^ 127)

/-- V3: the raw deletion passes its payload law; its stored absent successor fails. -/
theorem removed_payload_rejected :
    (decide ((RecordView.mk false 1).fields = 1) : Bool) = true ∧
    mergeGroup 0 ⟨true, 1⟩ [⟨false, 1⟩] = some ⟨false, 0⟩ ∧
    mergedLawful 0 ⟨true, 1⟩ [⟨false, 1⟩] (fun r => decide (r.fields = 1)) = false := by decide

theorem lawful_removal_and_canonical_transition :
    mergedLawful 0 ⟨true, 1⟩ [⟨false, 1⟩] (fun _ => true) = true ∧
    mergedLawful 0 ⟨true, 1⟩ [⟨false, 1⟩] (fun r => decide (r.fields = 0)) = true := by decide

theorem conflicting_aliases_rejected :
    mergeGroup 0 ⟨true, 0⟩ [⟨true, 1⟩, ⟨true, 2⟩] = none := by decide

theorem aliased_readonly_law_rechecked :
    (decide ((RecordView.mk true 0).fields = 0) : Bool) = true ∧
    mergeGroup 0 ⟨true, 0⟩ [⟨true, 0⟩, ⟨true, 1⟩] = some ⟨true, 1⟩ ∧
    mergedLawful 0 ⟨true, 0⟩ [⟨true, 0⟩, ⟨true, 1⟩] (fun r => decide (r.fields = 0)) = false := by decide

theorem self_transfer_mint_rejected :
    mergeGroup 0 ⟨true, 1⟩ [⟨true, 1⟩, ⟨true, 2⟩] = some ⟨true, 2⟩ ∧
    summaryDelta 1 1 1 2 = false := by decide

theorem missing_supply_increment_rejected : summaryDelta 0 0 0 1 = false := by decide

theorem summary_delta_cannot_be_dropped :
    total (fun n : Nat => n) ([] : List (Bool × Nat)) = 0 ∧
    total (fun n : Nat => n) (applyWrites [] [(false, some 1)]) ≠ 0 := by decide

theorem noncanonical_genesis_rejected :
    genesisAccepted 0 ⟨false, 2⟩ 0 [] = false ∧
    genesisAccepted 1 ⟨false, 0⟩ 0 [] = false ∧
    genesisAccepted 1 ⟨false, 1⟩ 0 [] = true ∧
    genesisAccepted 0 ⟨true, 0⟩ 0 [] = false ∧
    genesisAccepted 0 ⟨false, 0⟩ 2 [] = false := by decide

theorem bad_initial_condition_rejected :
    genesisAccepted 0 ⟨false, 0⟩ 0 [fun r => decide (r.fields = 2)] = false ∧
    genesisAccepted 0 ⟨false, 0⟩ 0 [fun r => decide (r.fields = 0)] = true ∧
    mergedLawful 0 ⟨false, 0⟩ [⟨true, 1⟩] (fun _ => true) = true := by decide

theorem absent_default_does_not_bind_stored_key :
    boundPostKey [] (some 0) = false ∧ boundPostKey [0] (some 0) = true := by decide

theorem false_undefined_and_vacuous_relations :
    transitionVerdict 0 0 0 = .unknown ∧
    transitionVerdict 1 0 1 = .unknown ∧
    transitionVerdict 2 1 1 = .refuted ∧
    transitionVerdict 1 0 0 = .checked := by decide

theorem signed_overflow_rejected :
    signedFits (2 ^ 127 - 1) = true ∧ signedFits (2 ^ 127) = false ∧
    signedFits (-(2 ^ 127) - 1) = false := by decide

theorem token_bound_one_is_not_inductive :
    (tokenCase 0 none none false true true 2).committed = true ∧
    ¬ balance (tokenCase 0 none none false true true 2).receiver ≤ 1 := by decide

theorem library_zero_return_is_inadmissible :
    ¬ copiesFor false (some (true, false)) ≤ loans (some 0) ∧
    (libraryCase (some 0) (some (true, false)) false 3 true).refused = true := by decide

#print axioms p1_follows_from_p2
#print axioms p2_hidden_read_counterexample
#print axioms p3_role_counterexample
#print axioms p4_unnamed_write_counterexample
#print axioms t1_fixed_group_counterexample
#print axioms t2_negative_coverage_counterexample
#print axioms t3_nonempty_genesis_counterexample
#print axioms t4_missing_target_delta_counterexample
#print axioms removed_payload_rejected
#print axioms lawful_removal_and_canonical_transition
#print axioms conflicting_aliases_rejected
#print axioms aliased_readonly_law_rechecked
#print axioms self_transfer_mint_rejected
#print axioms missing_supply_increment_rejected
#print axioms summary_delta_cannot_be_dropped
#print axioms noncanonical_genesis_rejected
#print axioms bad_initial_condition_rejected
#print axioms absent_default_does_not_bind_stored_key
#print axioms false_undefined_and_vacuous_relations
#print axioms signed_overflow_rejected
#print axioms token_bound_one_is_not_inductive
#print axioms library_zero_return_is_inadmissible
end CollectionsValidationWitnesses
