import CollectionsReductionGenesis
import CollectionsReductionTallies

/-! Executable specification fixtures and concrete non-vacuity witnesses. -/
namespace CollectionsReductionValidation
open CollectionsAggregate CollectionsReduction CollectionsReductionGenesis CollectionsReductionTallies

instance optionForall {A : Type} (p : Option A → Prop) [DecidablePred p]
    [Decidable (∀ a : A, p (some a))] : Decidable (∀ a : Option A, p a) :=
  decidable_of_iff (p none ∧ ∀ a : A, p (some a))
    ⟨fun h a => by cases a with | none => exact h.1 | some a => exact h.2 a,
      fun h => ⟨h none, fun a => h (some a)⟩⟩

def balance : Option (Fin 4) → Nat := contribution Fin.val

theorem balance_bound (record : Option (Fin 4)) : balance record < 4 := by
  cases record with
  | none => decide
  | some r => exact r.isLt

structure TokenResult where
  supply : Fin 7
  sender : Option (Fin 4)
  receiver : Option (Fin 4)
  committed : Bool

/-- The shipped token policy, with alias merge applied to its successor. -/
def tokenCase (supply : Fin 7) (sender receiver : Option (Fin 4))
    (alias mint issuer : Bool) (amount : Fin 4) : TokenResult :=
  let reject : TokenResult := ⟨supply, sender, receiver, false⟩
  if mint then
    if issuer then
      if h : supply.val + amount.val < 7 ∧ balance receiver + amount.val < 4 then
        let value : Option (Fin 4) := some ⟨balance receiver + amount.val, h.2⟩
        ⟨⟨supply.val + amount.val, h.1⟩, if alias then value else sender, value, true⟩
      else reject
    else reject
  else if sender.isNone then reject
  else if amount.val > balance sender then reject
  else if alias then ⟨supply, sender, receiver, true⟩
  else if h : balance receiver + amount.val < 4 then
    let bound := balance_bound sender
    ⟨supply, some ⟨balance sender - amount.val, by omega⟩,
      some ⟨balance receiver + amount.val, h⟩, true⟩
  else reject

def namedBalance (sender receiver : Option (Fin 4)) (alias : Bool) : Nat :=
  if alias then balance sender else balance sender + balance receiver

def TokenObligations (supply : Fin 7) (sender receiver : Option (Fin 4))
    (alias mint issuer : Bool) (amount : Fin 4) : Prop :=
  let result := tokenCase supply sender receiver alias mint issuer amount
  ((alias = true → sender = receiver) ∧ namedBalance sender receiver alias ≤ supply.val) →
    (mint = true → result.committed = true → issuer = true) ∧
    (mint = false → result.committed = true → sender.isSome = true) ∧
    (alias = true → result.sender = result.receiver) ∧
    result.supply.val + namedBalance sender receiver alias =
      supply.val + namedBalance result.sender result.receiver alias

instance tokenObligationsDecidable (supply : Fin 7) (sender receiver : Option (Fin 4))
    (alias mint issuer : Bool) (amount : Fin 4) :
    Decidable (TokenObligations supply sender receiver alias mint issuer amount) := by
  unfold TokenObligations
  infer_instance

theorem token_cases_checked :
    ∀ supply : Fin 7, ∀ sender receiver : Option (Fin 4),
    ∀ alias mint issuer : Bool, ∀ amount : Fin 4,
      TokenObligations supply sender receiver alias mint issuer amount := by
  decide

structure TokenInput where
  mint : Bool
  issuer : Bool
  amount : Fin 4

abbrev TokenView := View (fun _ : Fin 1 => Fin 4) (Fin 7) TokenInput 1 (fun _ => 2)
abbrev TokenUpdate := Update (fun _ : Fin 1 => Fin 4) (Fin 7) 1 (fun _ => 2)
abbrev TokenState := State (fun _ : Fin 1 => Bool) (fun _ => Fin 4) (Fin 7) 1

def tokenResult (v : TokenView) : TokenResult :=
  tokenCase v.scalar (v.entry 0 0) (v.entry 0 1) (v.same 0 0 1)
    v.input.mint v.input.issuer v.input.amount

def tokenCore (v : TokenView) : TokenUpdate :=
  let result := tokenResult v
  ⟨result.supply, fun _ => result.supply.val,
    fun _ i => if i = 0 then result.sender else result.receiver⟩

def tokenP (supply : Fin 7) (summary : Fin 1 → Nat) : Prop := summary 0 = supply.val
def tokenQ (_ : Fin 1) (_ : Option (Fin 4)) : Prop := True
def tokenLawful (v : TokenView) (_ : TokenUpdate) : Prop :=
  (v.input.mint = true → (tokenResult v).committed = true → v.input.issuer = true) ∧
  (v.input.mint = false → (tokenResult v).committed = true → (v.entry 0 0).isSome = true)

/-- The finite token computation discharges the abstract checked-contract premise. -/
theorem token_checked : Checked tokenP tokenQ (fun _ => 0) (fun _ => Fin.val)
    tokenLawful tokenCore := by
  intro v reps hr ha
  obtain ⟨hp, _, hrefl, hsymm, _, hagree, hbound⟩ := ha
  have before := two_representative_sum (v.same 0) (v.entry 0) Fin.val (reps 0)
    (hrefl 0) (hsymm 0) (hr.1 0) (hr.2 0) (hagree 0)
  have coverage : namedBalance (v.entry 0 0) (v.entry 0 1) (v.same 0 0 1) ≤ v.scalar.val := by
    have h := hbound 0
    rw [before] at h
    change v.summary 0 = v.scalar.val at hp
    simpa only [namedBalance, balance, hp] using h
  have cases := token_cases_checked v.scalar (v.entry 0 0) (v.entry 0 1) (v.same 0 0 1)
    v.input.mint v.input.issuer v.input.amount ⟨hagree 0 0 1, coverage⟩
  have post_agree : ∀ i j, v.same 0 i j = true → (tokenCore v).entry 0 i = (tokenCore v).entry 0 j := by
    simp only [Fin.forall_fin_two]
    refine ⟨⟨fun _ => True.intro, ?_⟩, ⟨?_, fun _ => True.intro⟩⟩
    · exact cases.2.2.1
    · intro h
      exact (cases.2.2.1 (hsymm 0 1 0 h)).symm
  have after := two_representative_sum (v.same 0) ((tokenCore v).entry 0) Fin.val (reps 0)
    (hrefl 0) (hsymm 0) (hr.1 0) (hr.2 0) post_agree
  refine ⟨⟨cases.1, cases.2.1⟩, rfl, fun _ _ => True.intro, ?_, ?_⟩
  · simpa only [Fin.forall_fin_one] using post_agree
  · simp only [Fin.forall_fin_one]
    rw [before, after]
    change v.summary 0 = v.scalar.val at hp
    rw [hp]
    simpa [tokenCore, tokenResult, namedBalance, balance] using cases.2.2.2

def tokenBinding (keys : Fin 2 → Bool) : Binding (fun _ : Fin 1 => Bool) (fun _ => 2) :=
  binding (fun _ => keys)

def tokenRun (keys : Nat → Fin 2 → Bool) (inputs : Nat → TokenInput) : Nat → TokenState
  | 0 => genesis 0
  | n + 1 =>
    let before := tokenRun keys inputs n
    let bind := tokenBinding (keys n)
    commit before bind (tokenCore (frame before bind (inputs n)))

/-- V1: every C1 premise is instantiated by the shipped token transition table. -/
example (keys : Nat → Fin 2 → Bool) (inputs : Nat → TokenInput) :
    ∀ n, Laws tokenP tokenQ (tokenRun keys inputs n) ∧
      Summaries (fun _ => 0) (fun _ => Fin.val) (tokenRun keys inputs n) ∧
      tokenLawful (frame (tokenRun keys inputs n) (tokenBinding (keys n)) (inputs n))
        (tokenCore (frame (tokenRun keys inputs n) (tokenBinding (keys n)) (inputs n))) := by
  apply run_preserves tokenP tokenQ (fun _ => 0) (fun _ => Fin.val) (fun _ _ => 2)
    (fun _ => tokenLawful) (fun _ => tokenCore) (fun _ => token_checked)
    (tokenRun keys inputs) (fun n => tokenBinding (keys n)) inputs
  · intro m; rfl
  · intro j; rfl
  · exact ⟨rfl, fun _ => True.intro⟩
  · intro n; exact ⟨rfl, rfl, fun _ _ => rfl⟩

/-- The explicit P2/P3/P4 boundary premises hold for the same token model. -/
example (keys : Nat → Fin 2 → Bool) (inputs : Nat → TokenInput) :
    ∀ n, Laws tokenP tokenQ (tokenRun keys inputs n) ∧
      Summaries (fun _ => 0) (fun _ => Fin.val) (tokenRun keys inputs n) ∧
      tokenLawful (frame (tokenRun keys inputs n) (tokenBinding (keys n)) (inputs n))
        (tokenCore (frame (tokenRun keys inputs n) (tokenBinding (keys n)) (inputs n))) ∧
      (∀ m i j, (tokenBinding (keys n)).key m i = (tokenBinding (keys n)).key m j →
        (tokenCore (frame (tokenRun keys inputs n) (tokenBinding (keys n)) (inputs n))).entry m i =
          (tokenCore (frame (tokenRun keys inputs n) (tokenBinding (keys n)) (inputs n))).entry m j) := by
  apply collections_reduction tokenP tokenQ (fun _ => 0) (fun _ => Fin.val) (fun _ _ => 2)
    (fun _ => tokenLawful) (fun _ => tokenCore) (fun _ state b input => tokenCore (frame state b input))
    (fun _ => token_checked) (fun _ _ _ _ => rfl) (Laws tokenP tokenQ) (fun _ => Iff.rfl)
    (tokenRun keys inputs) (fun n => tokenBinding (keys n)) inputs
  · intro m; rfl
  · intro j; rfl
  · exact ⟨rfl, fun _ => True.intro⟩
  · intro n; exact ⟨rfl, rfl, fun _ _ => rfl⟩

def tokenGenesisLaws : List (StateView (fun _ : Fin 1 => Fin 4) (Fin 7) 1 (fun _ => 2) → Bool) :=
  [fun v => decide (v.scalar = 0),
   fun v => !((v.entry 0 0).present) && decide ((v.entry 0 0).fields = 0),
   fun v => !((v.entry 0 1).present) && decide ((v.entry 0 1).fields = 0),
   fun v => decide (v.summary 0 = 0)]

/-- V1: C2 on the token empty-ledger initial conditions and absent-record law. -/
example (keys : Fin 2 → Bool) (input : TokenInput) :
    ∀ law, law ∈ tokenGenesisLaws → law (stateView (fun _ => 0)
      (frame (genesis (sums := 1) (0 : Fin 7)) (tokenBinding keys) input)) = true := by
  exact (genesis_check_iff tokenGenesisLaws (fun _ => 0) 0 (tokenBinding keys) input).mp (by decide)

example : (tokenCase 0 none none false true true 2).committed = true ∧
    (tokenCase 2 (some 2) none false false false 1).sender = some 1 ∧
    (tokenCase 2 (some 2) none false false false 1).receiver = some 1 := by decide

abbrev Copy := Bool × Bool

def loans : Option (Fin 3) → Nat := contribution Fin.val
def copiesFor (member : Bool) : Option Copy → Nat :=
  contribution (groupWeight Prod.snd (fun _ => 1) Prod.fst member)

structure LibraryResult where
  member : Option (Fin 3)
  copy : Option Copy
  committed : Bool
  refused : Bool

/-- Join, AddCopy, Borrow, Return, in the shipped rule order. An out-of-range
return has no committed successor; the tally precondition excludes that view. -/
def libraryCase (member : Option (Fin 3)) (copy : Option Copy)
    (key : Bool) (action : Fin 4) (librarian : Bool) : LibraryResult :=
  let reject : LibraryResult := ⟨member, copy, false, false⟩
  if !librarian then reject
  else if action = 0 then
    if member.isSome then reject else ⟨some 0, copy, true, false⟩
  else if action = 1 then
    if copy.isSome then reject else ⟨member, some (false, key), true, false⟩
  else match member, copy with
    | none, _ => reject
    | _, none => reject
    | some count, some (onLoan, holder) =>
      if action = 2 then
        if onLoan then reject
        else if h : count.val < 2 then
          ⟨some ⟨count.val + 1, by omega⟩, some (true, key), true, false⟩
        else reject
      else if !onLoan || decide (holder ≠ key) then reject
      else if count.val = 0 then ⟨member, copy, false, true⟩
      else ⟨some ⟨count.val - 1, by have h := count.isLt; omega⟩,
        some (false, holder), true, false⟩

instance pairForall {A B : Type} (p : A × B → Prop) [DecidablePred p]
    [Decidable (∀ a : A, ∀ b : B, p (a, b))] : Decidable (∀ x : A × B, p x) :=
  decidable_of_iff (∀ a : A, ∀ b : B, p (a, b))
    ⟨fun h ⟨a, b⟩ => h a b, fun h a b => h (a, b)⟩

def LibraryObligations (member : Option (Fin 3)) (copy : Option Copy)
    (key : Bool) (action : Fin 4) (librarian : Bool) : Prop :=
  let result := libraryCase member copy key action librarian
  copiesFor key copy ≤ loans member →
    result.refused = false ∧
    (result.committed = true → librarian = true) ∧
    (action = 2 → result.committed = true → result.copy = some (true, key)) ∧
    (action = 3 → result.committed = true → copy = some (true, key) ∧ result.copy = some (false, key)) ∧
    ∀ group, (if group = key then loans result.member else 0) + copiesFor group copy =
      (if group = key then loans member else 0) + copiesFor group result.copy

instance libraryObligationsDecidable (member : Option (Fin 3)) (copy : Option Copy)
    (key : Bool) (action : Fin 4) (librarian : Bool) :
    Decidable (LibraryObligations member copy key action librarian) := by
  unfold LibraryObligations
  infer_instance

theorem library_cases_checked :
    ∀ member : Option (Fin 3), ∀ copy : Option Copy, ∀ key : Bool,
    ∀ action : Fin 4, ∀ librarian : Bool,
      LibraryObligations member copy key action librarian := by decide

def libraryTargetWrites (key : Bool) (result : LibraryResult) : List (Bool × Option (Fin 3)) :=
  [(key, result.member)]

def librarySourceWrites (key : Fin 2) (result : LibraryResult) : List (Fin 2 × Option Copy) :=
  [(key, result.copy)]

/-- The exact finite policy table supplies T4 whenever the reachable tally
invariant supplies the checker's named-contribution lower bound. -/
theorem library_delta (target : List (Bool × Fin 3)) (source : List (Fin 2 × Copy))
    (memberKey : Bool) (copyKey : Fin 2) (action : Fin 4) (librarian : Bool)
    (coverage : copiesFor memberKey (lookup copyKey source) ≤ loans (lookup memberKey target)) :
    let result := libraryCase (lookup memberKey target) (lookup copyKey source) memberKey action librarian
    DeltaChecked Fin.val Prod.snd (fun _ => 1) Prod.fst target source
      (libraryTargetWrites memberKey result) (librarySourceWrites copyKey result) := by
  dsimp only
  intro key _
  have h := (library_cases_checked (lookup memberKey target) (lookup copyKey source)
    memberKey action librarian coverage).2.2.2.2 key
  simp only [libraryTargetWrites, librarySourceWrites, applyWrites, namedTotal,
    writtenTotal, Nat.add_zero]
  by_cases he : key = memberKey
  · subst key
    simpa only [lookup_put_same, if_true, loans, copiesFor] using h
  · rw [lookup_put_other _ _ _ _ he]
    simp only [he, if_false, Nat.zero_add] at h
    exact congrArg (fun n => contribution Fin.val (lookup key target) + n) h

def libraryRun (inputs : Nat → Bool × Fin 2 × Fin 4 × Bool) :
    Nat → List (Bool × Fin 3) × List (Fin 2 × Copy)
  | 0 => ([], [])
  | n + 1 =>
    let previous := libraryRun inputs n
    let input := inputs n
    let result := libraryCase (lookup input.1 previous.1) (lookup input.2.1 previous.2)
      input.1 input.2.2.1 input.2.2.2
    (applyWrites previous.1 (libraryTargetWrites input.1 result),
      applyWrites previous.2 (librarySourceWrites input.2.1 result))

theorem library_run_unique (inputs : Nat → Bool × Fin 2 × Fin 4 × Bool) :
    ∀ n, Unique (libraryRun inputs n).2 := by
  intro n
  induction n with
  | zero => exact List.nodup_nil
  | succ n ih => exact unique_applyWrites _ _ ih

theorem library_coverage (target : List (Bool × Fin 3)) (source : List (Fin 2 × Copy))
    (unique : Unique source)
    (invariant : Invariant Fin.val Prod.snd (fun _ => 1) Prod.fst target source)
    (memberKey : Bool) (copyKey : Fin 2) :
    copiesFor memberKey (lookup copyKey source) ≤ loans (lookup memberKey target) := by
  have h := namedTotal_le (groupWeight Prod.snd (fun _ => 1) Prod.fst memberKey)
    source [(copyKey, none)] unique (by simp [Unique, keys])
  rw [← invariant memberKey] at h
  simpa only [namedTotal, Nat.add_zero, loans, copiesFor] using h

theorem library_run_tally (inputs : Nat → Bool × Fin 2 × Fin 4 × Bool) :
    ∀ n, Invariant Fin.val Prod.snd (fun _ => 1) Prod.fst
      (libraryRun inputs n).1 (libraryRun inputs n).2 := by
  intro n
  induction n with
  | zero => exact genesis_tally Fin.val Prod.snd (fun _ => 1) Prod.fst
  | succ n ih =>
    apply tally_step Fin.val Prod.snd (fun _ => 1) Prod.fst
      (libraryRun inputs n).1 (libraryRun inputs n).2 _ _ (library_run_unique inputs n)
      (by simp [librarySourceWrites, Unique, keys]) ih
    exact library_delta _ _ _ _ _ _ (library_coverage _ _ (library_run_unique inputs n) ih _ _)

/-- The shipped journey: join, add, borrow, return, followed by visitor rejections. -/
def libraryJourney : Nat → Bool × Fin 2 × Fin 4 × Bool
  | 0 => (false, 0, 0, true)
  | 1 => (false, 0, 1, true)
  | 2 => (false, 0, 2, true)
  | 3 => (false, 0, 3, true)
  | _ + 4 => (false, 0, 0, false)

def libraryWitness : Nat → List (Bool × Fin 3) × List (Fin 2 × Copy)
  | 0 => ([], [])
  | 1 => ([(false, 0)], [])
  | 2 => ([(false, 0)], [(0, (false, false))])
  | 3 => ([(false, 1)], [(0, (true, false))])
  | _ + 4 => ([(false, 0)], [(0, (false, false))])

def libraryWitnessResult (n : Nat) : LibraryResult :=
  libraryCase (lookup (libraryJourney n).1 (libraryWitness n).1)
    (lookup (libraryJourney n).2.1 (libraryWitness n).2)
    (libraryJourney n).1 (libraryJourney n).2.2.1 (libraryJourney n).2.2.2

def libraryWitnessTargetWrites (n : Nat) := libraryTargetWrites false (libraryWitnessResult n)
def libraryWitnessSourceWrites (n : Nat) := librarySourceWrites 0 (libraryWitnessResult n)

/-- V1: all C3 run premises reduce to five concrete cases of the shipped journey. -/
example : ∀ n, Invariant Fin.val Prod.snd (fun _ => 1) Prod.fst
    (libraryWitness n).1 (libraryWitness n).2 := by
  apply tally_run Fin.val Prod.snd (fun _ => 1) Prod.fst
    (fun n => (libraryWitness n).1) (fun n => (libraryWitness n).2)
    libraryWitnessTargetWrites libraryWitnessSourceWrites rfl rfl
  · intro n
    rcases n with (_ | _ | _ | _ | n) <;> simp [libraryWitness, Unique, keys]
  · intro n
    simp [libraryWitnessSourceWrites, librarySourceWrites, Unique, keys]
  · intro n
    rcases n with (_ | _ | _ | _ | n) <;>
      dsimp [libraryWitness, libraryWitnessTargetWrites, libraryWitnessResult, libraryJourney] <;> decide
  · intro n
    rcases n with (_ | _ | _ | _ | n) <;>
      dsimp [libraryWitness, libraryWitnessSourceWrites, libraryWitnessResult, libraryJourney] <;> decide
  · intro n
    rcases n with (_ | _ | _ | _ | n) <;>
      dsimp [libraryWitness, libraryWitnessTargetWrites, libraryWitnessSourceWrites,
        libraryWitnessResult, libraryJourney, DeltaChecked] <;> decide

def LibraryRecords (m : Fin 2) : Type := if m = 0 then Fin 3 else Copy
def LibraryKeys (m : Fin 2) : Type := if m = 0 then Bool else Fin 2

instance libraryKeysEq (m : Fin 2) : DecidableEq (LibraryKeys m) := by
  unfold LibraryKeys
  split <;> infer_instance

instance libraryRecordsEq (m : Fin 2) : DecidableEq (LibraryRecords m) := by
  unfold LibraryRecords
  split <;> infer_instance

def libraryLeast (m : Fin 2) : LibraryRecords m :=
  if h : m = 0 then (by simp only [LibraryRecords, h, if_true]; exact 0)
  else (by simp only [LibraryRecords, h, if_false]; exact (false, false))

def libraryGenesisLaws : List (StateView LibraryRecords Unit 0 (fun _ => 1) → Bool) :=
  [fun v => (v.entry 0 0).present || decide ((v.entry 0 0).fields = (0 : Fin 3))]

/-- V1: C2 for the library's actual genesis-applicable absent-member law. -/
example (b : Binding LibraryKeys (fun _ => 1)) :
    ∀ law, law ∈ libraryGenesisLaws →
      law (stateView libraryLeast (frame (genesis (sums := 0) ()) b ())) = true := by
  exact (genesis_check_iff libraryGenesisLaws libraryLeast () b ()).mp (by decide)

#print axioms token_cases_checked
#print axioms token_checked
#print axioms library_cases_checked
#print axioms library_delta
#print axioms library_run_tally
end CollectionsReductionValidation
