import Lean

/-!
Development gate: inspect the kernel environment of a freshly compiled module.
The owning module, not a source-text declaration pattern, selects constants.
Private declarations, instances, definitions and generated auxiliaries are included.
This runner is part of the trusted development tooling, not a proof module.
-/
open Lean

instance : MonadEnv (StateM Environment) where
  getEnv := get
  modifyEnv := modify

def allowedAxioms : Array Name := #[`propext, `Quot.sound, `Classical.choice]

def constantKind : ConstantInfo → String
  | .axiomInfo _ => "axiom"
  | .defnInfo _ => "definition"
  | .thmInfo _ => "theorem"
  | .opaqueInfo _ => "opaque"
  | .quotInfo _ => "quotient"
  | .inductInfo _ => "inductive"
  | .ctorInfo _ => "constructor"
  | .recInfo _ => "recursor"

def main (args : List String) : IO UInt32 := do
  let [moduleText] := args
    | throw (IO.userError "usage: AuditModule.lean MODULE")
  let moduleName := moduleText.toName
  initSearchPath (← findSysroot)
  -- Private import visibility preserves all declaration bodies. Extension
  -- initializers are unnecessary: ownership and axiom traversal use kernel data.
  let env ← importModules #[{ module := moduleName }] {} (trustLevel := 0)
    (loadExts := false) (level := .private)
  let owned := env.constants.toList.filter fun (name, _) =>
    match env.getModuleIdxFor? name with
    | none => false
    | some index => env.header.moduleNames[index.toNat]? == some moduleName
  let owned := owned.toArray.qsort (fun a b => a.1.toString < b.1.toString)
  let mut records : Array Json := #[]
  let mut failures := 0
  for (name, info) in owned do
    -- Public API also used by #print axioms (including imported dependency
    -- metadata on Lean versions that retain it at module compilation).
    let dependencies := ((collectAxioms name : StateM Environment (Array Name)).run env).1
    let axioms := dependencies.qsort (fun a b => a.toString < b.toString)
    let forbidden := axioms.filter (!allowedAxioms.contains ·)
    if !forbidden.isEmpty || info.isUnsafe then
      failures := failures + 1
    records := records.push (Json.mkObj [
      ("name", toJson name.toString), ("kind", toJson (constantKind info)),
      ("axioms", toJson (axioms.map Name.toString)),
      ("forbidden", toJson (forbidden.map Name.toString)),
      ("unsafe", toJson info.isUnsafe)])
  let accepted := !owned.isEmpty && failures == 0
  IO.println (Json.mkObj [
    ("schema", toJson "zeno-fcis/lean-module-audit/1"),
    ("module", toJson moduleText), ("constant_count", toJson owned.size),
    ("rejected_count", toJson failures), ("accepted", toJson accepted),
    ("constants", Json.arr records)]).compress
  return if accepted then 0 else 1
