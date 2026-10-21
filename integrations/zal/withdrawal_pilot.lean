-- Withdrawal-queue pilot, appended to the exported claim inside namespace
-- ZenoFCIS: the controller model, the contract rows it must equal, and the
-- unbounded response theorem under the fairness premise A. Authority: none.
-- Generated rows replace the four table markers; nothing else is generated.
-- The liveness marker separates the model from the response proof.
namespace WithdrawalPilot

inductive Lane where
  | empty | arrived | pending
  deriving DecidableEq, Repr

inductive Side where
  | a | b
  deriving DecidableEq, Repr

inductive Pause where
  | p0 | p1 | p2
  deriving DecidableEq, Repr

structure Ctrl where
  la : Lane
  lb : Lane
  pause : Pause
  must : Bool
  prio : Side
  deriving DecidableEq, Repr

inductive Act where
  | tick (alarm : Bool)
  | request (side : Side)
  | idle
  deriving DecidableEq, Repr

structure Obs where
  ra : Bool
  ca : Bool
  rb : Bool
  cb : Bool
  deriving DecidableEq, Repr

def quiet : Obs := ⟨false, false, false, false⟩

def due : Lane → Bool
  | .empty => false
  | _ => true

def present : Lane → Lane
  | .arrived => .pending
  | other => other

/-- Rule 6, restated: a paused tick counts down and sets must-serve when the
last paused tick ends with a lane due; an alarm outside a pause and outside
must-serve starts a pause of two more ticks; otherwise the priority lane, or
the only due lane, is paid and priority moves to the other lane. -/
def tick (c : Ctrl) (alarm : Bool) : Ctrl × Obs :=
  match c.pause with
  | .p2 => ({ c with la := present c.la, lb := present c.lb, pause := .p1, must := false }, quiet)
  | .p1 => ({ c with la := present c.la, lb := present c.lb, pause := .p0, must := due c.la || due c.lb }, quiet)
  | .p0 =>
    if alarm && !c.must then
      ({ c with la := present c.la, lb := present c.lb, pause := .p2, must := false }, quiet)
    else if due c.la && (!due c.lb || c.prio == .a) then
      ({ la := .empty, lb := present c.lb, pause := .p0, must := false, prio := .b }, ⟨false, true, false, false⟩)
    else if due c.lb then
      ({ la := present c.la, lb := .empty, pause := .p0, must := false, prio := .a }, ⟨false, false, false, true⟩)
    else
      ({ c with must := false }, quiet)

/-- One decision: a keeper tick, an accepted owner request, or anything else
(deposits and rejections), which leaves the controller part unchanged. -/
def dstep (c : Ctrl) : Act → Ctrl × Obs
  | .tick alarm => tick c alarm
  | .request .a => if c.la = .empty then ({ c with la := .arrived }, ⟨true, false, false, false⟩) else (c, quiet)
  | .request .b => if c.lb = .empty then ({ c with lb := .arrived }, ⟨false, false, true, false⟩) else (c, quiet)
  | .idle => (c, quiet)

def init : Ctrl := ⟨.empty, .empty, .p0, false, .a⟩

def allActs : List Act := [.tick false, .tick true, .request .a, .request .b, .idle]

def sides : List Side := [.a, .b]

/-- Every distinct controller row of the contract's checked decisions. -/
def contractRows : List (Ctrl × Act × Ctrl × Obs) := @@ROWS@@

/-- The controller part of every vault reachable from the genesis. -/
def reach : List Ctrl := @@REACH@@

theorem rows_agree :
    contractRows.all (fun r => dstep r.1 r.2.1 == (r.2.2.1, r.2.2.2)) = true := by decide +kernel

theorem init_reach : reach.contains init = true := by decide +kernel

theorem reach_closed :
    reach.all (fun c => allActs.all (fun a => reach.contains (dstep c a).1)) = true := by decide +kernel

def lane (c : Ctrl) : Side → Lane
  | .a => c.la
  | .b => c.lb

def occupied (s : Side) (c : Ctrl) : Bool := lane c s != .empty

def paid (s : Side) (o : Obs) : Bool :=
  match s with
  | .a => o.ca
  | .b => o.cb

def requested (s : Side) (o : Obs) : Bool :=
  match s with
  | .a => o.ra
  | .b => o.rb

theorem all_true {α : Type} {p : α → Bool} {l : List α} (h : l.all p = true) {x : α}
    (hx : x ∈ l) : p x = true :=
  List.all_eq_true.mp h x hx

theorem mem_of_contains {c : Ctrl} (h : reach.contains c = true) : c ∈ reach := by
  simpa using h

theorem act_mem (a : Act) : a ∈ allActs := by
  cases a with
  | tick al => cases al <;> simp [allActs]
  | request s => cases s <;> simp [allActs]
  | idle => simp [allActs]

theorem side_mem (s : Side) : s ∈ sides := by cases s <;> simp [sides]

theorem bool_mem (b : Bool) : b ∈ [false, true] := by cases b <;> simp

/-- The controller after each decision of an action sequence from the genesis. -/
def ctrlAt (act : Nat → Act) : Nat → Ctrl
  | 0 => init
  | n + 1 => (dstep (ctrlAt act n) (act n)).1

def obsAt (act : Nat → Act) (n : Nat) : Obs := (dstep (ctrlAt act n) (act n)).2

theorem ctrlAt_succ (act : Nat → Act) (n : Nat) :
    ctrlAt act (n + 1) = (dstep (ctrlAt act n) (act n)).1 := rfl

theorem ctrlAt_reach (act : Nat → Act) : ∀ n, ctrlAt act n ∈ reach
  | 0 => mem_of_contains init_reach
  | n + 1 => by
    rw [ctrlAt_succ]
    exact mem_of_contains (all_true (all_true reach_closed (ctrlAt_reach act n)) (act_mem (act n)))

def isTick : Act → Bool
  | .tick _ => true
  | _ => false

/-- A(sigma): the keeper's tick is taken infinitely often. -/
def Fair (act : Nat → Act) : Prop := ∀ n, ∃ m, n ≤ m ∧ isTick (act m) = true

def bit (b : Bool) : I128 :=
  if b then ⟨1, by unfold inI128 i128Min i128Max; omega⟩
  else ⟨0, by unfold inI128 i128Min i128Max; omega⟩

theorem bit_one (b : Bool) : (bit b).val = (1 : Int) ↔ b = true := by
  cases b <;> simp [bit]

/-- The observation stream of a run: requests and completions per lane, at
the paths the lowered claim reads. -/
def observeRun (act : Nat → Act) (name : String) (t : Nat) : I128 :=
  if name = "pre_100_110" then bit (obsAt act t).ra
  else if name = "pre_100_111" then bit (obsAt act t).ca
  else if name = "pre_100_112" then bit (obsAt act t).rb
  else if name = "pre_100_113" then bit (obsAt act t).cb
  else bit false

-- @@LIVENESS@@
/-- Worst-case ticks until each occupied lane is paid. The kernel checks the
two properties below, so the table's origin carries no trust. -/
def potA : List (Ctrl × Nat) := @@POT_A@@

def potB : List (Ctrl × Nat) := @@POT_B@@

def pot (s : Side) (c : Ctrl) : Nat :=
  (List.lookup c (match s with | .a => potA | .b => potB)).getD 0

def tickProgress (c : Ctrl) (s : Side) (alarm : Bool) : Bool :=
  !occupied s c || paid s (dstep c (.tick alarm)).2 ||
    (occupied s (dstep c (.tick alarm)).1 && decide (pot s (dstep c (.tick alarm)).1 < pot s c))

def otherKeeps (c : Ctrl) (s : Side) (a : Act) : Bool :=
  !occupied s c || (occupied s (dstep c a).1 && decide (pot s (dstep c a).1 ≤ pot s c))

def requestOccupies (c : Ctrl) (s : Side) (a : Act) : Bool :=
  !requested s (dstep c a).2 || occupied s (dstep c a).1

theorem tick_progress_all :
    reach.all (fun c => sides.all (fun s => [false, true].all (fun al => tickProgress c s al))) = true := by
  decide +kernel

theorem other_keeps_all :
    reach.all (fun c => sides.all (fun s =>
      [Act.request .a, .request .b, .idle].all (fun a => otherKeeps c s a))) = true := by
  decide +kernel

theorem request_occupies_all :
    reach.all (fun c => sides.all (fun s => allActs.all (fun a => requestOccupies c s a))) = true := by
  decide +kernel

theorem tick_step {c : Ctrl} (hc : c ∈ reach) (s : Side) (al : Bool) (ho : occupied s c = true) :
    paid s (dstep c (.tick al)).2 = true ∨
      (occupied s (dstep c (.tick al)).1 = true ∧ pot s (dstep c (.tick al)).1 < pot s c) := by
  have h : tickProgress c s al = true :=
    all_true (all_true (all_true tick_progress_all hc) (side_mem s)) (bool_mem al)
  simp only [tickProgress, ho] at h
  simpa using h

theorem other_step {c : Ctrl} (hc : c ∈ reach) (s : Side) {a : Act} (hn : isTick a = false)
    (ho : occupied s c = true) :
    occupied s (dstep c a).1 = true ∧ pot s (dstep c a).1 ≤ pot s c := by
  have ha : a ∈ [Act.request .a, .request .b, .idle] := by
    cases a with
    | tick _ => simp [isTick] at hn
    | request s => cases s <;> simp
    | idle => simp
  have h : otherKeeps c s a = true := all_true (all_true (all_true other_keeps_all hc) (side_mem s)) ha
  simp only [otherKeeps, ho] at h
  simpa using h

theorem eventually_paid (act : Nat → Act) (fair : Fair act) (s : Side) :
    ∀ b n, occupied s (ctrlAt act n) = true → pot s (ctrlAt act n) < b →
      ∃ m, n ≤ m ∧ paid s (obsAt act m) = true := by
  intro b
  induction b with
  | zero => intro n _ h; exact absurd h (Nat.not_lt_zero _)
  | succ b ih =>
    intro n hocc hpot
    -- One decision either pays the lane, lowers its potential, or keeps both.
    have step : ∀ j, n ≤ j → occupied s (ctrlAt act j) = true →
        pot s (ctrlAt act j) ≤ pot s (ctrlAt act n) →
        (∃ m, n ≤ m ∧ paid s (obsAt act m) = true) ∨
          (isTick (act j) = false ∧ occupied s (ctrlAt act (j + 1)) = true ∧
            pot s (ctrlAt act (j + 1)) ≤ pot s (ctrlAt act n)) := by
      intro j hnj hj hle
      have hr := ctrlAt_reach act j
      cases hact : act j with
      | tick al =>
        rcases tick_step hr s al hj with hpaid | ⟨hocc', hlt⟩
        · exact Or.inl ⟨j, hnj, by simpa [obsAt, hact] using hpaid⟩
        · have e : ctrlAt act (j + 1) = (dstep (ctrlAt act j) (.tick al)).1 := by
            rw [ctrlAt_succ, hact]
          obtain ⟨m, hm, hp⟩ := ih (j + 1) (by rw [e]; exact hocc') (by rw [e]; omega)
          exact Or.inl ⟨m, by omega, hp⟩
      | request side =>
        have e : ctrlAt act (j + 1) = (dstep (ctrlAt act j) (.request side)).1 := by
          rw [ctrlAt_succ, hact]
        obtain ⟨ho', hle'⟩ := other_step hr s (a := .request side) rfl hj
        exact Or.inr ⟨rfl, by rw [e]; exact ho', by rw [e]; omega⟩
      | idle =>
        have e : ctrlAt act (j + 1) = (dstep (ctrlAt act j) .idle).1 := by
          rw [ctrlAt_succ, hact]
        obtain ⟨ho', hle'⟩ := other_step hr s (a := .idle) rfl hj
        exact Or.inr ⟨rfl, by rw [e]; exact ho', by rw [e]; omega⟩
    have walk : ∀ d, (∃ m, n ≤ m ∧ paid s (obsAt act m) = true) ∨
        (occupied s (ctrlAt act (n + d)) = true ∧
          pot s (ctrlAt act (n + d)) ≤ pot s (ctrlAt act n)) := by
      intro d
      induction d with
      | zero => exact Or.inr ⟨hocc, Nat.le_refl _⟩
      | succ d ihd =>
        rcases ihd with done | ⟨ho, hle⟩
        · exact Or.inl done
        · rcases step (n + d) (by omega) ho hle with done | ⟨_, ho', hle'⟩
          · exact Or.inl done
          · have e : n + (d + 1) = n + d + 1 := by omega
            rw [e]
            exact Or.inr ⟨ho', hle'⟩
    obtain ⟨m0, hnm0, htick⟩ := fair n
    rcases walk (m0 - n) with done | ⟨ho, hle⟩
    · exact done
    · have e : n + (m0 - n) = m0 := by omega
      rw [e] at ho hle
      rcases step m0 hnm0 ho hle with done | ⟨hnt, _, _⟩
      · exact done
      · rw [htick] at hnt
        exact absurd hnt (by decide)

theorem responds (act : Nat → Act) (fair : Fair act) (s : Side) (t : Nat)
    (h : requested s (obsAt act t) = true) : ∃ m, t ≤ m ∧ paid s (obsAt act m) = true := by
  have hocc : occupied s (ctrlAt act (t + 1)) = true := by
    have hr : requestOccupies (ctrlAt act t) s (act t) = true :=
      all_true (all_true (all_true request_occupies_all (ctrlAt_reach act t)) (side_mem s))
        (act_mem (act t))
    have h' : requested s (dstep (ctrlAt act t) (act t)).2 = true := h
    simp only [requestOccupies, h'] at hr
    rw [ctrlAt_succ]
    simpa using hr
  obtain ⟨m, hm, hp⟩ :=
    eventually_paid act fair s (pot s (ctrlAt act (t + 1)) + 1) (t + 1) hocc (Nat.lt_succ_self _)
  exact ⟨m, by omega, hp⟩

/-- Provided A, an alarm may delay a withdrawal but never freeze it: every
run of the model from the genesis in which the keeper's tick is taken
infinitely often satisfies the exported claim, exactly as exported. -/
theorem withdrawal_never_freezes (act : Nat → Act) (fair : Fair act)
    (preds : String → List Int → Prop) : claim_2 (observeRun act) preds := by
  unfold claim_2
  refine ⟨by simp, fun t _ => ⟨?_, fun u _ => ?_⟩⟩
  · cases hr : (obsAt act t).ra with
    | true =>
      obtain ⟨m, hm, hp⟩ := responds act fair .a t (by simpa [requested] using hr)
      exact Or.inr ⟨m, hm, by simpa [observeRun, bit_one, paid] using hp⟩
    | false => exact Or.inl (by simp [observeRun, bit_one, hr])
  · cases hr : (obsAt act u).rb with
    | true =>
      obtain ⟨m, hm, hp⟩ := responds act fair .b u (by simpa [requested] using hr)
      exact Or.inr ⟨m, hm, by simpa [observeRun, bit_one, paid] using hp⟩
    | false => exact Or.inl (by simp [observeRun, bit_one, hr])

end WithdrawalPilot

#print axioms WithdrawalPilot.withdrawal_never_freezes

end ZenoFCIS
