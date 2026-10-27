import Lean
import CollectionsPatternsExecutable

/-! Native table export, outside the kernel-proof inventory. -/
open Lean CollectionsPatternsExecutable

deriving instance FromJson for Domain
deriving instance FromJson for KeyPosition
deriving instance FromJson for Entry
deriving instance FromJson for Summary
deriving instance FromJson for Tally
deriving instance FromJson for Shape

def tupleText (tuple : Tuple) : String := String.join (tuple.map toString)

def main (args : List String) : IO UInt32 := do
  let [source, destination, receipt] := args
    | throw (IO.userError "usage: CollectionsListing.lean SHAPES LISTING COUNTS")
  let input ← IO.FS.Handle.mk source .read
  let output ← IO.FS.Handle.mk destination .write
  let mut shapes := 0
  let mut candidates := 0
  let mut accepted := 0
  repeat
    let line ← input.getLine
    if line.isEmpty then break
    let json ← IO.ofExcept (Json.parse line)
    let shape ← IO.ofExcept (fromJson? json : Except String Shape)
    let domain := tuples shape.domains
    let selected := domain.filter (structuralAdmissible shape)
    candidates := candidates + domain.length
    accepted := accepted + selected.length
    output.putStr (toString shapes ++ ":" ++ String.join (selected.map (fun tuple => tupleText tuple ++ ",")) ++ "\n")
    shapes := shapes + 1
  output.flush
  let counts := Json.mkObj [("schema", toJson "zeno-fcis/collections-reduction-counts/1"),
    ("shapes", toJson shapes), ("tuples", toJson candidates), ("admitted", toJson accepted)]
  IO.FS.writeFile receipt (counts.compress ++ "\n")
  IO.println counts.compress
  return 0
