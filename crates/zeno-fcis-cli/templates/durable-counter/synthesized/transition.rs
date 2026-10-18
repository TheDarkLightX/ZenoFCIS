// Generated from zeno-fcis/finite-i128/1. Pure code; no commit authority.
/// Evaluates the generated finite program; no commit authority.
#[rustfmt::skip]
pub fn transition(input: &[i128]) -> Option<[i128; 6]> {
    if input.len() != 4 { return None; }
    if !(0_i128..=3_i128).contains(&input[0]) { return None; }
    if !(0_i128..=3_i128).contains(&input[1]) { return None; }
    if !(0_i128..=1_i128).contains(&input[2]) { return None; }
    if !(0_i128..=1_i128).contains(&input[3]) { return None; }
    let v0: i128 = input[0];
    let _ = v0;
    let v1: i128 = input[1];
    let _ = v1;
    let v2: i128 = input[2];
    let _ = v2;
    let v3: i128 = input[3];
    let _ = v3;
    let v4: i128 = if v2 == 1 { v1 } else { v0 };
    let _ = v4;
    let v5: i128 = 3_i128;
    let _ = v5;
    let v6: i128 = 1_i128;
    let _ = v6;
    let v7: i128 = 0_i128;
    let _ = v7;
    let v8: i128 = 1_i128;
    let _ = v8;
    let v9: i128 = 2_i128;
    let _ = v9;
    let v10: i128 = 3_i128;
    let _ = v10;
    let v11: i128 = i128::from(v4 < v5);
    let _ = v11;
    let v12: i128 = i128::from(v3 == 1 && v11 == 1);
    let _ = v12;
    let v13: i128 = v0.checked_add(v6)?;
    let _ = v13;
    let v14: i128 = v1.checked_add(v6)?;
    let _ = v14;
    let v15: i128 = if v2 == 1 { v0 } else { v13 };
    let _ = v15;
    let v16: i128 = if v12 == 1 { v15 } else { v0 };
    let _ = v16;
    let v17: i128 = if v2 == 1 { v14 } else { v1 };
    let _ = v17;
    let v18: i128 = if v12 == 1 { v17 } else { v1 };
    let _ = v18;
    let v19: i128 = if v2 == 1 { v10 } else { v9 };
    let _ = v19;
    let v20: i128 = if v11 == 1 { v19 } else { v8 };
    let _ = v20;
    let v21: i128 = if v3 == 1 { v20 } else { v7 };
    let _ = v21;
    let v22: i128 = if v12 == 1 { v16 } else { v7 };
    let _ = v22;
    let v23: i128 = if v12 == 1 { v18 } else { v7 };
    let _ = v23;
    if !(0_i128..=3_i128).contains(&v21) { return None; }
    if !(0_i128..=3_i128).contains(&v16) { return None; }
    if !(0_i128..=3_i128).contains(&v18) { return None; }
    if !(0_i128..=1_i128).contains(&v12) { return None; }
    if !(0_i128..=3_i128).contains(&v22) { return None; }
    if !(0_i128..=3_i128).contains(&v23) { return None; }
    Some([v21,v16,v18,v12,v22,v23,])
}
