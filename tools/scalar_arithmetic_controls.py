"""Body-only arithmetic proof faults, with independently chosen accepting witnesses.

These are unqualified specimens until the real proof/native suites run. A
frontend error or nonzero subprocess exit is never a semantic mutant kill.
"""
from pathlib import Path
import re

SCALAR=Path("crates/zeno-fcis-synthesis/src/finite/value/scalar.rs")
WIDE=SCALAR.with_name("wide.rs")
TYPED=Path("crates/zeno-fcis-synthesis/src/finite/evaluation/typed.rs")

# name, path, anchor, replacement, intended function, independent witness
CONTROLS=(
    ("scalar_truncation",SCALAR,"let rounded = if r != 0 && (up != negative) { q + 1 } else { q };","let rounded = q;","divide",[-1,2]),
    ("scalar_swapped_rounding",TYPED,"Instruction::ScalarDiv(false, a, b)","Instruction::ScalarDiv(true, a, b)","legacy_instruction",[5,2]),
    ("scalar_zero_divisor_to_zero",SCALAR,"if b == 0 { return None; }","if b == 0 { return Some(0); }","divide",[1,0]),
    ("scalar_wrapping_multiplication",SCALAR,"a.checked_mul(b)","match (a as i128).checked_mul(b as i128) { Some(product) => { #[cfg(verus_keep_ghost)] let wrapped = #[verifier::truncate] (product as i64); #[cfg(not(verus_keep_ghost))] let wrapped = product as i64; Some(wrapped) }, None => None }","multiply",[9223372036854775807,2]),
    ("scalar_missing_final_fit",SCALAR,"Some(q) if q <= i64::MAX as u128","Some(q)","mul_div",[9223372036854775807,9223372036854775807,1]),
    ("scalar_zero_quotient",SCALAR,"Some(q as i64)","Some(0)","mul_div",[11,1,10]),
    ("scalar_always_refuse",SCALAR,"Some(q as i64)","None","mul_div",[9223372036854775807,9223372036854775807,9223372036854775807]),
    ("scalar_dropped_product_word",WIDE,"p00 % RADIX + (middle % RADIX) * RADIX","p00 % RADIX","multiply",[9223372036854775807,9223372036854775807,9223372036854775807]),
)


def replace_once(source:str,before:str,after:str)->str:
    tokens=re.findall(r"\w+|[^\w\s]",before)
    matches=list(re.finditer(r"\s*".join(re.escape(t) for t in tokens),source))
    if len(matches)!=1:
        raise ValueError("arithmetic mutation anchor is not unique: "+before)
    match=matches[0]
    return source[:match.start()]+after+source[match.end():]


def specimens(root:Path)->dict:
    return {name:(path,replace_once((root/path).read_text(),before,after),"proof")
        for name,path,before,after,_,_ in CONTROLS}


def targets()->dict:
    return {name:function for name,_,_,_,function,_ in CONTROLS}
