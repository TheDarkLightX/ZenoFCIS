import Collections

/-!
Finite partial maps over an arbitrary decidable key type. Absence has no
stored payload. A summary uses a nonnegative contribution from each present
record. The list representation has unique keys and imposes no size bound.
-/
namespace CollectionsAggregate

variable {K R : Type} [DecidableEq K]

def keys (rows : List (K × R)) : List K := rows.map Prod.fst

omit [DecidableEq K] in
theorem keys_cons (key : K) (value : R) (rows : List (K × R)) :
    keys ((key, value) :: rows) = key :: keys rows := rfl

def Unique (rows : List (K × R)) : Prop := (keys rows).Nodup

def lookup (key : K) : List (K × R) → Option R
  | [] => none
  | (k, r) :: rows => if key = k then some r else lookup key rows

def erase (key : K) : List (K × R) → List (K × R)
  | [] => []
  | (k, r) :: rows => if key = k then erase key rows else (k, r) :: erase key rows

def put (key : K) (value : Option R) (rows : List (K × R)) : List (K × R) :=
  match value with
  | none => erase key rows
  | some r => (key, r) :: erase key rows

def contribution (amount : R → Nat) : Option R → Nat
  | none => 0
  | some r => amount r

def total (amount : R → Nat) : List (K × R) → Nat
  | [] => 0
  | (_, r) :: rows => amount r + total amount rows

theorem lookup_none_of_not_mem (key : K) (rows : List (K × R))
    (h : key ∉ keys rows) : lookup key rows = none := by
  induction rows with
  | nil => rfl
  | cons head rows ih =>
    rcases head with ⟨k, r⟩
    have h' : key ≠ k ∧ key ∉ keys rows := by simpa [keys] using h
    simp [lookup, h'.1, ih h'.2]

theorem erase_of_not_mem (key : K) (rows : List (K × R))
    (h : key ∉ keys rows) : erase key rows = rows := by
  induction rows with
  | nil => rfl
  | cons head rows ih =>
    rcases head with ⟨k, r⟩
    have h' : key ≠ k ∧ key ∉ keys rows := by simpa [keys] using h
    simp [erase, h'.1, ih h'.2]

theorem mem_keys_erase (key k : K) (rows : List (K × R)) :
    k ∈ keys (erase key rows) ↔ k ∈ keys rows ∧ k ≠ key := by
  induction rows with
  | nil => simp [erase, keys]
  | cons head rows ih =>
    rcases head with ⟨a, r⟩
    by_cases h : key = a
    · rw [erase, if_pos h, keys_cons, List.mem_cons, ih]
      subst a
      by_cases hk : k = key <;> simp [hk]
    · simp only [erase, if_neg h, keys_cons, List.mem_cons]
      rw [ih]
      constructor
      · intro hk
        rcases hk with hka | ⟨hkr, hkn⟩
        · exact ⟨Or.inl hka, fun hk' => h (hk'.symm.trans hka)⟩
        · exact ⟨Or.inr hkr, hkn⟩
      · rintro ⟨hka | hkr, hkn⟩
        · exact Or.inl hka
        · exact Or.inr ⟨hkr, hkn⟩

theorem unique_erase (key : K) (rows : List (K × R)) (h : Unique rows) :
    Unique (erase key rows) := by
  induction rows with
  | nil => exact h
  | cons head rows ih =>
    rcases head with ⟨k, r⟩
    have hs : k ∉ keys rows ∧ Unique rows := by simpa [Unique, keys] using h
    by_cases hk : key = k
    · simpa [erase, hk] using ih hs.2
    · simp only [erase, if_neg hk]
      change (k :: keys (erase key rows)).Nodup
      exact List.nodup_cons.mpr ⟨fun hm => hs.1 ((mem_keys_erase key k rows).mp hm).1,
        ih hs.2⟩

theorem unique_put (key : K) (value : Option R) (rows : List (K × R))
    (h : Unique rows) : Unique (put key value rows) := by
  cases value with
  | none => exact unique_erase key rows h
  | some r =>
    change (key :: keys (erase key rows)).Nodup
    exact List.nodup_cons.mpr ⟨fun hm => ((mem_keys_erase key key rows).mp hm).2 rfl,
      unique_erase key rows h⟩

theorem lookup_erase_same (key : K) (rows : List (K × R)) :
    lookup key (erase key rows) = none := by
  exact lookup_none_of_not_mem key _ (fun h => ((mem_keys_erase key key rows).mp h).2 rfl)

theorem lookup_erase_other (key k : K) (rows : List (K × R)) (h : k ≠ key) :
    lookup k (erase key rows) = lookup k rows := by
  induction rows with
  | nil => rfl
  | cons head rows ih =>
    rcases head with ⟨a, r⟩
    by_cases ha : key = a
    · subst a
      simp [erase, lookup, h, ih]
    · by_cases hk : k = a
      · simp [erase, lookup, ha, hk]
      · simp [erase, lookup, ha, hk, ih]

theorem lookup_of_mem (rows : List (K × R)) (h : Unique rows) (key : K) (record : R)
    (hm : (key, record) ∈ rows) : lookup key rows = some record := by
  induction rows with
  | nil => simp at hm
  | cons head rows ih =>
    rcases head with ⟨k, r⟩
    have hs : k ∉ keys rows ∧ Unique rows := by simpa [Unique, keys] using h
    rcases List.mem_cons.mp hm with he | hr
    · cases he; simp [lookup]
    · have hn : key ≠ k := by
        intro he
        apply hs.1
        subst key
        exact List.mem_map.mpr ⟨(k, record), hr, rfl⟩
      simp only [lookup, if_neg hn]
      exact ih hs.2 hr

theorem lookup_put_same (key : K) (value : Option R) (rows : List (K × R)) :
    lookup key (put key value rows) = value := by
  cases value with
  | none => exact lookup_erase_same key rows
  | some r => simp [put, lookup]

theorem lookup_put_other (key k : K) (value : Option R) (rows : List (K × R))
    (h : k ≠ key) : lookup k (put key value rows) = lookup k rows := by
  cases value with
  | none => exact lookup_erase_other key k rows h
  | some r => simp [put, lookup, h, lookup_erase_other key k rows h]

theorem erase_total (amount : R → Nat) (key : K) (rows : List (K × R))
    (h : Unique rows) :
    total amount (erase key rows) + contribution amount (lookup key rows) = total amount rows := by
  induction rows with
  | nil => rfl
  | cons head rows ih =>
    rcases head with ⟨k, r⟩
    have hs : k ∉ keys rows ∧ Unique rows := by simpa [Unique, keys] using h
    by_cases hk : key = k
    · subst k
      simp [erase, lookup, contribution, total, erase_of_not_mem key rows hs.1, Nat.add_comm]
    · have ht := ih hs.2
      simp only [erase, if_neg hk, lookup, total]
      omega

/-- The aggregate belongs to the partial map, independently of list order. -/
theorem total_ext (amount : R → Nat) (a b : List (K × R))
    (ha : Unique a) (hb : Unique b) (same : ∀ k, lookup k a = lookup k b) :
    total amount a = total amount b := by
  induction a generalizing b with
  | nil =>
    cases b with
    | nil => rfl
    | cons head rest =>
      have h := same head.1
      simp [lookup] at h
  | cons head rest ih =>
    rcases head with ⟨key, record⟩
    have hs : key ∉ keys rest ∧ Unique rest := by simpa [Unique, keys] using ha
    have at_key : lookup key b = some record := by simpa [lookup] using (same key).symm
    have rest_same : ∀ k, lookup k rest = lookup k (erase key b) := by
      intro k
      by_cases h : k = key
      · subst k
        rw [lookup_none_of_not_mem key rest hs.1, lookup_erase_same]
      · rw [lookup_erase_other key k b h]
        simpa only [lookup, if_neg h] using same k
    have hr := ih (erase key b) hs.2 (unique_erase key b hb) rest_same
    have ht := erase_total amount key b hb
    rw [at_key] at ht
    simp only [total, contribution] at ht ⊢
    omega

theorem put_total (amount : R → Nat) (key : K) (value : Option R)
    (rows : List (K × R)) (h : Unique rows) :
    total amount (put key value rows) + contribution amount (lookup key rows) =
      total amount rows + contribution amount value := by
  have he := erase_total amount key rows h
  cases value with
  | none => simpa [put, contribution] using he
  | some r =>
    change amount r + total amount (erase key rows) + contribution amount (lookup key rows) =
      total amount rows + amount r
    omega

def applyWrites (rows : List (K × R)) : List (K × Option R) → List (K × R)
  | [] => rows
  | (key, value) :: rest => put key value (applyWrites rows rest)

def namedTotal (amount : R → Nat) (rows : List (K × R)) : List (K × Option R) → Nat
  | [] => 0
  | (key, _) :: rest => contribution amount (lookup key rows) + namedTotal amount rows rest

def writtenTotal (amount : R → Nat) : List (K × Option R) → Nat
  | [] => 0
  | (_, value) :: rest => contribution amount value + writtenTotal amount rest

theorem unique_applyWrites (rows : List (K × R)) (writes : List (K × Option R))
    (h : Unique rows) : Unique (applyWrites rows writes) := by
  induction writes with
  | nil => exact h
  | cons head rest ih => exact unique_put head.1 head.2 _ ih

theorem lookup_applyWrites_unnamed (rows : List (K × R)) (writes : List (K × Option R))
    (key : K) (h : key ∉ keys writes) :
    lookup key (applyWrites rows writes) = lookup key rows := by
  induction writes with
  | nil => rfl
  | cons head rest ih =>
    rcases head with ⟨k, value⟩
    have hs : key ≠ k ∧ key ∉ keys rest := by simpa [keys] using h
    rw [applyWrites, lookup_put_other k key value _ hs.1, ih hs.2]

theorem lookup_applyWrites_named (rows : List (K × R)) (writes : List (K × Option R))
    (h : Unique writes) (key : K) (value : Option R) (hm : (key, value) ∈ writes) :
    lookup key (applyWrites rows writes) = value := by
  induction writes with
  | nil => simp at hm
  | cons head rest ih =>
    rcases head with ⟨k, v⟩
    have hs : k ∉ keys rest ∧ Unique rest := by simpa [Unique, keys] using h
    rcases List.mem_cons.mp hm with he | hr
    · cases he
      exact lookup_put_same key value _
    · have hn : key ≠ k := by
        intro he
        apply hs.1
        subst key
        exact List.mem_map.mpr ⟨(k, value), hr, rfl⟩
      rw [applyWrites, lookup_put_other k key v _ hn]
      exact ih hs.2 hr

theorem namedTotal_congr (amount : R → Nat) (a b : List (K × R))
    (writes : List (K × Option R))
    (h : ∀ k, k ∈ keys writes → lookup k a = lookup k b) :
    namedTotal amount a writes = namedTotal amount b writes := by
  induction writes with
  | nil => rfl
  | cons head rest ih =>
    rcases head with ⟨key, value⟩
    have hk : key ∈ keys ((key, value) :: rest) := by simp [keys]
    have hr : ∀ k, k ∈ keys rest → lookup k a = lookup k b := by
      intro k hm
      exact h k (by simpa only [keys_cons, List.mem_cons] using Or.inr hm)
    simp only [namedTotal, h key hk, ih hr]

/-- Nonnegative named contributions cannot exceed the whole-map aggregate. -/
theorem namedTotal_le (amount : R → Nat) (rows : List (K × R))
    (writes : List (K × Option R)) (h : Unique rows) (hw : Unique writes) :
    namedTotal amount rows writes ≤ total amount rows := by
  induction writes generalizing rows with
  | nil => simp [namedTotal]
  | cons head rest ih =>
    rcases head with ⟨key, value⟩
    have hs : key ∉ keys rest ∧ Unique rest := by simpa [Unique, keys] using hw
    have hr := ih (erase key rows) (unique_erase key rows h) hs.2
    have hn : namedTotal amount (erase key rows) rest = namedTotal amount rows rest := by
      apply namedTotal_congr
      intro k hm
      apply lookup_erase_other
      intro he
      exact hs.1 (he ▸ hm)
    rw [hn] at hr
    have he := erase_total amount key rows h
    simp only [namedTotal]
    omega

/-- Uniform entry predicates are preserved at named and unnamed keys. -/
theorem writes_preserve (law : Option R → Prop) (rows : List (K × R))
    (writes : List (K × Option R)) (hw : Unique writes)
    (before : ∀ k, law (lookup k rows))
    (checked : ∀ k v, (k, v) ∈ writes → law v) :
    ∀ k, law (lookup k (applyWrites rows writes)) := by
  intro k
  by_cases hm : k ∈ keys writes
  · obtain ⟨⟨key, value⟩, hv, he⟩ := List.mem_map.mp hm
    change key = k at he
    subst key
    rw [lookup_applyWrites_named rows writes hw k value hv]
    exact checked k value hv
  · rw [lookup_applyWrites_unnamed rows writes k hm]
    exact before k

/-- The whole-map change is exactly the change at distinct written keys. -/
theorem writes_total (amount : R → Nat) (rows : List (K × R))
    (writes : List (K × Option R)) (h : Unique rows) (hw : Unique writes) :
    total amount (applyWrites rows writes) + namedTotal amount rows writes =
      total amount rows + writtenTotal amount writes := by
  induction writes with
  | nil => simp [applyWrites, namedTotal, writtenTotal]
  | cons head rest ih =>
    rcases head with ⟨key, value⟩
    have hs : key ∉ keys rest ∧ Unique rest := by simpa [Unique, keys] using hw
    have ht := ih hs.2
    have hp := put_total amount key value (applyWrites rows rest) (unique_applyWrites rows rest h)
    rw [lookup_applyWrites_unnamed rows rest key hs.1] at hp
    simp only [applyWrites, namedTotal, writtenTotal]
    omega

/-- A matching local summary delta preserves the aggregate identity. -/
theorem summary_step (amount : R → Nat) (rows : List (K × R))
    (writes : List (K × Option R)) (h : Unique rows) (hw : Unique writes)
    (before after : Nat) (initial : before = total amount rows)
    (delta : after + namedTotal amount rows writes = before + writtenTotal amount writes) :
    after = total amount (applyWrites rows writes) := by
  have htotal := writes_total amount rows writes h hw
  omega

#print axioms writes_total
#print axioms summary_step
#print axioms namedTotal_le
#print axioms writes_preserve
end CollectionsAggregate
