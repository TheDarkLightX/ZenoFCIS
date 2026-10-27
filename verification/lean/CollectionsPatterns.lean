import Collections

/-! Restricted-growth strings, independently of any bound on key positions. -/
namespace CollectionsPatterns

def Grows (next : Nat) : List Nat → Prop
  | [] => True
  | a :: rest => a ≤ next ∧ Grows (max next (a + 1)) rest

instance growsDecidable (next : Nat) (xs : List Nat) : Decidable (Grows next xs) := by
  cases xs with
  | nil => exact isTrue True.intro
  | cons a rest =>
    have := growsDecidable (max next (a + 1)) rest
    exact inferInstanceAs (Decidable (a ≤ next ∧ Grows (max next (a + 1)) rest))

def enumerate (next : Nat) : Nat → List (List Nat)
  | 0 => [[]]
  | n + 1 => (List.range (next + 1)).flatMap (fun a =>
      (enumerate (max next (a + 1)) n).map (List.cons a))

theorem enumerate_iff (next n : Nat) (xs : List Nat) :
    xs ∈ enumerate next n ↔ xs.length = n ∧ Grows next xs := by
  induction n generalizing next xs with
  | zero =>
    constructor
    · intro h
      have he : xs = [] := List.mem_singleton.mp h
      subst xs
      exact ⟨rfl, True.intro⟩
    · intro ⟨h, _⟩
      have he : xs = [] := List.length_eq_zero_iff.mp h
      subst xs
      exact List.mem_cons_self
  | succ n ih =>
    constructor
    · intro h
      obtain ⟨a, ha, hm⟩ := List.mem_flatMap.mp h
      obtain ⟨rest, hr, he⟩ := List.mem_map.mp hm
      subst xs
      have ht := (ih _ rest).mp hr
      exact ⟨congrArg Nat.succ ht.1, ⟨Nat.le_of_lt_succ (List.mem_range.mp ha), ht.2⟩⟩
    · intro ⟨hl, hg⟩
      cases xs with
      | nil => simp only [List.length_nil] at hl; omega
      | cons a rest =>
        have hn : rest.length = n := by simpa only [List.length_cons, Nat.add_right_cancel_iff] using hl
        exact List.mem_flatMap.mpr ⟨a, List.mem_range.mpr (Nat.lt_succ_of_le hg.1),
          List.mem_map.mpr ⟨rest, (ih _ rest).mpr ⟨hn, hg.2⟩, rfl⟩⟩

theorem range_nodup (n : Nat) : (List.range n).Nodup := by
  induction n with
  | zero => exact List.nodup_nil
  | succ n ih =>
    rw [List.range_succ, List.Nodup, List.pairwise_append]
    refine ⟨ih, by simp only [List.pairwise_singleton], ?_⟩
    intro a ha b hb
    have he : b = n := List.mem_singleton.mp hb
    have hn := List.mem_range.mp ha
    omega

theorem enumerate_nodup (next n : Nat) : (enumerate next n).Nodup := by
  induction n generalizing next with
  | zero => simp [enumerate]
  | succ n ih =>
    rw [enumerate, List.Nodup, List.pairwise_flatMap]
    constructor
    · intro a _
      rw [List.pairwise_map]
      exact (ih (max next (a + 1))).imp (fun h he => h (List.cons.inj he).2)
    · apply (range_nodup (next + 1)).imp
      intro a b h x hx y hy he
      obtain ⟨xs, _, rfl⟩ := List.mem_map.mp hx
      obtain ⟨ys, _, rfl⟩ := List.mem_map.mp hy
      exact h (List.cons.inj he).1

variable {A B : Type} [DecidableEq A] [DecidableEq B]

/-- Constructive versions avoid classical dependencies in generic search lemmas. -/
theorem index_self (xs : List A) (a : A) : (a :: xs).idxOf a = 0 := by
  simp only [List.idxOf_cons, cond_eq_ite, beq_iff_eq, if_true]

theorem index_not_mem (xs : List A) (a : A) (h : a ∉ xs) : xs.idxOf a = xs.length := by
  induction xs with
  | nil => rfl
  | cons x rest ih =>
    have hx : x ≠ a := fun he => h (by subst a; exact List.mem_cons_self)
    have hr : a ∉ rest := fun hm => h (List.mem_cons_of_mem x hm)
    simp only [List.idxOf_cons, cond_eq_ite, beq_iff_eq, hx, if_false, ih hr, List.length_cons]

theorem index_le (xs : List A) (a : A) : xs.idxOf a ≤ xs.length := by
  induction xs with
  | nil => exact Nat.le_refl 0
  | cons x rest ih =>
    simp only [List.idxOf_cons, cond_eq_ite, beq_iff_eq, List.length_cons]
    split <;> omega

theorem index_lt_iff (xs : List A) (a : A) : xs.idxOf a < xs.length ↔ a ∈ xs := by
  constructor
  · intro ht
    by_cases hm : a ∈ xs
    · exact hm
    · rw [index_not_mem xs a hm] at ht
      omega
  · exact List.idxOf_lt_length_of_mem

theorem index_append (xs ys : List A) (a : A) :
    (xs ++ ys).idxOf a = if a ∈ xs then xs.idxOf a else ys.idxOf a + xs.length := by
  induction xs with
  | nil => simp only [List.nil_append, List.not_mem_nil, if_false, List.length_nil, Nat.add_zero]
  | cons x rest ih =>
    by_cases hx : x = a
    · subst x
      simp only [List.cons_append, List.idxOf_cons_self, List.mem_cons_self, if_true]
    · have ha : a ≠ x := Ne.symm hx
      simp only [List.cons_append, List.idxOf_cons, cond_eq_ite, beq_iff_eq, hx, if_false,
        List.mem_cons, ha, false_or, List.length_cons, ih]
      split <;> omega

def advance (seen : List A) (a : A) : List A :=
  if a ∈ seen then seen else seen ++ [a]

def finish (seen : List A) : List A → List A
  | [] => seen
  | a :: rest => finish (advance seen a) rest

def encode (seen : List A) : List A → List Nat
  | [] => []
  | a :: rest => seen.idxOf a :: encode (advance seen a) rest

def normalize (xs : List A) := encode [] xs

theorem advance_length (seen : List A) (a : A) :
    (advance seen a).length = max seen.length (seen.idxOf a + 1) := by
  by_cases h : a ∈ seen
  · have bound := List.idxOf_lt_length_of_mem h
    simp only [advance, h, if_true]
    omega
  · rw [advance, if_neg h, List.length_append, List.length_singleton, index_not_mem seen a h]
    omega

theorem encode_grows (seen xs : List A) : Grows seen.length (encode seen xs) := by
  induction xs generalizing seen with
  | nil => exact True.intro
  | cons a rest ih =>
    refine ⟨index_le seen a, ?_⟩
    rw [← advance_length]
    exact ih (advance seen a)

theorem encode_length (seen xs : List A) : (encode seen xs).length = xs.length := by
  induction xs generalizing seen with
  | nil => rfl
  | cons a rest ih => simp only [encode, List.length_cons, ih]

theorem finish_prefix (seen xs : List A) : ∃ tail, finish seen xs = seen ++ tail := by
  induction xs generalizing seen with
  | nil => exact ⟨[], (List.append_nil seen).symm⟩
  | cons a rest ih =>
    obtain ⟨tail, he⟩ := ih (advance seen a)
    by_cases h : a ∈ seen
    · exact ⟨tail, by simpa only [finish, advance, h, if_true] using he⟩
    · exact ⟨a :: tail, by simpa only [finish, advance, h, if_false, List.append_assoc,
        List.singleton_append] using he⟩

theorem mem_finish_left (seen xs : List A) (a : A) (h : a ∈ seen) : a ∈ finish seen xs := by
  obtain ⟨tail, he⟩ := finish_prefix seen xs
  rw [he]
  exact List.mem_append_left tail h

theorem mem_advance (seen : List A) (a : A) : a ∈ advance seen a := by
  by_cases h : a ∈ seen <;> simp [advance, h]

theorem mem_finish_input (seen xs : List A) (a : A) (h : a ∈ xs) : a ∈ finish seen xs := by
  induction xs generalizing seen with
  | nil => simp at h
  | cons x rest ih =>
    rcases List.mem_cons.mp h with he | hr
    · subst a
      exact mem_finish_left (advance seen x) rest x (mem_advance seen x)
    · exact ih (advance seen x) hr

theorem idx_finish (seen xs : List A) (a : A) (h : a ∈ seen) :
    (finish seen xs).idxOf a = seen.idxOf a := by
  obtain ⟨tail, he⟩ := finish_prefix seen xs
  rw [he, index_append, if_pos h]

theorem idx_advance (seen : List A) (a : A) : (advance seen a).idxOf a = seen.idxOf a := by
  by_cases h : a ∈ seen
  · simp [advance, h]
  · simp only [advance, h, if_false, index_append, List.idxOf_cons_self,
      Nat.zero_add, index_not_mem seen a h]

theorem encode_map_finish (seen xs : List A) :
    encode seen xs = xs.map (fun a => (finish seen xs).idxOf a) := by
  induction xs generalizing seen with
  | nil => rfl
  | cons a rest ih =>
    simp only [encode, List.map_cons, finish, ih]
    rw [idx_finish (advance seen a) rest a (mem_advance seen a), idx_advance]

theorem idx_injective (xs : List A) (a b : A) (ha : a ∈ xs) (hb : b ∈ xs)
    (same : xs.idxOf a = xs.idxOf b) : a = b := by
  induction xs with
  | nil => simp at ha
  | cons x rest ih =>
    by_cases xa : x = a
    · by_cases xb : x = b
      · exact xa.symm.trans xb
      · have hab : a ≠ b := fun he => xb (xa.trans he)
        simp only [List.idxOf_cons, cond_eq_ite, beq_iff_eq, xa, hab, if_true, if_false] at same
        omega
    · by_cases xb : x = b
      · have hba : b ≠ a := fun he => xa (xb.trans he)
        simp only [List.idxOf_cons, cond_eq_ite, beq_iff_eq, xb, hba, if_true, if_false] at same
        omega
      · have har : a ∈ rest := (List.mem_cons.mp ha).resolve_left (Ne.symm xa)
        have hbr : b ∈ rest := (List.mem_cons.mp hb).resolve_left (Ne.symm xb)
        apply ih har hbr
        simpa only [List.idxOf_cons, cond_eq_ite, beq_iff_eq, xa, xb, if_false,
          Nat.add_right_cancel_iff] using same

inductive Aligned (relation : A → B → Prop) : List A → List B → Prop
  | nil : Aligned relation [] []
  | cons : relation a b → Aligned relation xs ys → Aligned relation (a :: xs) (b :: ys)

omit [DecidableEq A] [DecidableEq B] in
theorem aligned_length (rel : A → B → Prop) (xs : List A) (ys : List B)
    (h : Aligned rel xs ys) : xs.length = ys.length := by
  induction h with
  | nil => rfl
  | cons _ _ ih => exact congrArg Nat.succ ih

omit [DecidableEq A] [DecidableEq B] in
theorem aligned_append (rel : A → B → Prop) (xs : List A) (ys : List B)
    (as' : List A) (bs : List B) (h : Aligned rel xs ys) (tail : Aligned rel as' bs) :
    Aligned rel (xs ++ as') (ys ++ bs) := by
  induction h with
  | nil => exact tail
  | cons head _ ih => exact Aligned.cons head ih

/-- A partial bijection relating the keys at corresponding positions. -/
def Coherent (rel : A → B → Prop) : Prop :=
  ∀ a b a' b', rel a b → rel a' b' → (a = a' ↔ b = b')

def SamePattern (xs : List A) (ys : List B) : Prop :=
  ∃ rel : A → B → Prop, Aligned rel xs ys ∧ Coherent rel

theorem idx_aligned (rel : A → B → Prop) (coherent : Coherent rel)
    (xs : List A) (ys : List B) (h : Aligned rel xs ys) (a : A) (b : B) (ab : rel a b) :
    xs.idxOf a = ys.idxOf b := by
  induction h with
  | nil => rfl
  | @cons x y xs ys xy _ ih =>
    have he := coherent x y a b xy ab
    by_cases hx : x = a
    · simp only [hx, he.mp hx, List.idxOf_cons_self]
    · have hy : y ≠ b := fun h => hx (he.mpr h)
      simp only [List.idxOf_cons, cond_eq_ite, beq_iff_eq, hx, hy, if_false, ih]

theorem encode_aligned (rel : A → B → Prop) (coherent : Coherent rel)
    (seen : List A) (other : List B) (seen_rel : Aligned rel seen other)
    (xs : List A) (ys : List B) (h : Aligned rel xs ys) :
    encode seen xs = encode other ys := by
  induction h generalizing seen other with
  | nil => rfl
  | @cons a b xs ys ab tail ih =>
    have hi := idx_aligned rel coherent seen other seen_rel a b ab
    have hl := aligned_length rel seen other seen_rel
    have hm : a ∈ seen ↔ b ∈ other := by
      rw [← index_lt_iff, ← index_lt_iff, hi, hl]
    have advanced : Aligned rel (advance seen a) (advance other b) := by
      by_cases ha : a ∈ seen
      · simpa only [advance, ha, hm.mp ha, if_true] using seen_rel
      · have hb : b ∉ other := fun h => ha (hm.mpr h)
        simpa only [advance, ha, hb, if_false] using
          aligned_append rel seen other [a] [b] seen_rel (Aligned.cons ab Aligned.nil)
    simp only [encode, hi, ih _ _ advanced]

theorem normalize_pattern (xs : List A) : SamePattern xs (normalize xs) := by
  let table := finish [] xs
  let rel : A → Nat → Prop := fun a i => a ∈ table ∧ table.idxOf a = i
  refine ⟨rel, ?_, ?_⟩
  · rw [normalize, encode_map_finish]
    have build : ∀ ys : List A, (∀ a, a ∈ ys → a ∈ table) →
        Aligned rel ys (ys.map (fun a => table.idxOf a)) := by
      intro ys
      induction ys with
      | nil => intro _; exact Aligned.nil
      | cons a rest ih =>
        intro h
        exact Aligned.cons ⟨h a List.mem_cons_self, rfl⟩
          (ih (fun a ha => h a (List.mem_cons_of_mem _ ha)))
    exact build xs (fun a ha => mem_finish_input [] xs a ha)
  · intro a b a' b' ha hb
    constructor
    · intro he
      exact ha.2.symm.trans ((congrArg (fun x => table.idxOf x) he).trans hb.2)
    · intro he
      exact idx_injective table a a' ha.1 hb.1 (ha.2.trans (he.trans hb.2.symm))

theorem idx_range (n a : Nat) : (List.range n).idxOf a = min a n := by
  induction n with
  | zero => simp only [List.range_zero, List.idxOf_nil, Nat.min_zero]
  | succ n ih =>
    rw [List.range_succ, index_append]
    by_cases h : a < n
    · rw [if_pos (List.mem_range.mpr h), ih,
        Nat.min_eq_left (Nat.le_of_lt h), Nat.min_eq_left (Nat.le_succ_of_le (Nat.le_of_lt h))]
    · by_cases he : a = n
      · subst a
        simp only [List.mem_range, Nat.lt_irrefl, if_false, index_self, Nat.zero_add,
          List.length_range]
        exact (Nat.min_eq_left (Nat.le_succ n)).symm
      · have hn : n ≠ a := Ne.symm he
        have lower : n + 1 ≤ a := by omega
        simp only [List.mem_range, h, if_false, List.idxOf_cons, cond_eq_ite, beq_iff_eq,
          hn, List.idxOf_nil, Nat.zero_add, List.length_range]
        simpa only [Nat.add_comm] using (Nat.min_eq_right lower).symm

theorem encode_range (next : Nat) (xs : List Nat) (grows : Grows next xs) :
    encode (List.range next) xs = xs := by
  induction xs generalizing next with
  | nil => rfl
  | cons a rest ih =>
    have hindex : (List.range next).idxOf a = a := by
      rw [idx_range]
      exact Nat.min_eq_left grows.1
    have hadvance : advance (List.range next) a = List.range (max next (a + 1)) := by
      by_cases h : a < next
      · have hm : max next (a + 1) = next := by omega
        simp [advance, List.mem_range, h, hm]
      · have he : a = next := by have := grows.1; omega
        subst a
        have hm : max next (next + 1) = next + 1 := by omega
        simp [advance, List.mem_range, hm, List.range_succ]
    simp only [encode, hindex, hadvance, ih _ grows.2]

theorem normalize_fixed (xs : List Nat) (grows : Grows 0 xs) : normalize xs = xs :=
  encode_range 0 xs grows

theorem pattern_unique (xs : List A) (a b : List Nat)
    (ga : Grows 0 a) (gb : Grows 0 b)
    (pa : SamePattern xs a) (pb : SamePattern xs b) : a = b := by
  obtain ⟨ra, ha, ca⟩ := pa
  obtain ⟨rb, hb, cb⟩ := pb
  have ea := encode_aligned ra ca [] [] Aligned.nil xs a ha
  have eb := encode_aligned rb cb [] [] Aligned.nil xs b hb
  change normalize xs = normalize a at ea
  change normalize xs = normalize b at eb
  rw [normalize_fixed a ga] at ea
  rw [normalize_fixed b gb] at eb
  exact ea.symm.trans eb

/-- C4: exactly one enumerated representative for every concrete equality pattern. -/
theorem equality_pattern_coverage (xs : List A) :
    ∃ representative, representative ∈ enumerate 0 xs.length ∧ SamePattern xs representative ∧
      ∀ other, other ∈ enumerate 0 xs.length → SamePattern xs other → other = representative := by
  refine ⟨normalize xs, (enumerate_iff 0 xs.length _).mpr
    ⟨encode_length [] xs, encode_grows [] xs⟩, normalize_pattern xs, ?_⟩
  intro other ho hp
  exact pattern_unique xs other (normalize xs) ((enumerate_iff 0 xs.length other).mp ho).2
    (encode_grows [] xs) hp (normalize_pattern xs)

#print axioms enumerate_iff
#print axioms enumerate_nodup
#print axioms normalize_pattern
#print axioms pattern_unique
#print axioms equality_pattern_coverage
end CollectionsPatterns
