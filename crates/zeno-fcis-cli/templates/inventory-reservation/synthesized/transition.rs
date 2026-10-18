// Generated from zeno-fcis/finite-i128/1. Pure code; no commit authority.
/// Evaluates the generated finite program; no commit authority.
#[rustfmt::skip]
pub fn transition(input: &[i128]) -> Option<[i128; 4]> {
    if input.len() != 5 { return None; }
    if !(0_i128..=5_i128).contains(&input[0]) { return None; }
    if !(0_i128..=5_i128).contains(&input[1]) { return None; }
    if !(0_i128..=3_i128).contains(&input[2]) { return None; }
    if !(1_i128..=3_i128).contains(&input[3]) { return None; }
    if !(0_i128..=1_i128).contains(&input[4]) { return None; }
    let v0: i128 = input[0];
    let _ = v0;
    let v1: i128 = input[1];
    let _ = v1;
    let v2: i128 = input[2];
    let _ = v2;
    let v3: i128 = input[3];
    let _ = v3;
    let v4: i128 = 5_i128;
    let _ = v4;
    let v5: i128 = 1_i128;
    let _ = v5;
    let v6: i128 = 0_i128;
    let _ = v6;
    let v7: i128 = i128::from(v2 == v6);
    let _ = v7;
    let v8: i128 = 1_i128;
    let _ = v8;
    let v9: i128 = i128::from(v2 == v8);
    let _ = v9;
    let v10: i128 = 2_i128;
    let _ = v10;
    let v11: i128 = i128::from(v2 == v10);
    let _ = v11;
    let v12: i128 = i128::from(v0 < v3);
    let _ = v12;
    let v13: i128 = i128::from(v1 < v3);
    let _ = v13;
    let v14: i128 = v0.checked_add(v3)?;
    let _ = v14;
    let v15: i128 = v1.checked_add(v3)?;
    let _ = v15;
    let v16: i128 = v0.checked_sub(v3)?;
    let _ = v16;
    let v17: i128 = v1.checked_sub(v3)?;
    let _ = v17;
    let v18: i128 = i128::from(v4 < v15);
    let _ = v18;
    let v19: i128 = i128::from(v4 < v14);
    let _ = v19;
    let v20: i128 = 3_i128;
    let _ = v20;
    let v21: i128 = if v18 == 1 { v10 } else { v20 };
    let _ = v21;
    let v22: i128 = if v12 == 1 { v6 } else { v21 };
    let _ = v22;
    let v23: i128 = if v19 == 1 { v10 } else { v20 };
    let _ = v23;
    let v24: i128 = if v13 == 1 { v5 } else { v23 };
    let _ = v24;
    let v25: i128 = if v13 == 1 { v5 } else { v20 };
    let _ = v25;
    let v26: i128 = if v11 == 1 { v25 } else { v23 };
    let _ = v26;
    let v27: i128 = if v9 == 1 { v24 } else { v26 };
    let _ = v27;
    let v28: i128 = if v7 == 1 { v22 } else { v27 };
    let _ = v28;
    let v29: i128 = i128::from(v28 == v20);
    let _ = v29;
    let v30: i128 = if v11 == 1 { v0 } else { v14 };
    let _ = v30;
    let v31: i128 = if v9 == 1 { v14 } else { v30 };
    let _ = v31;
    let v32: i128 = if v7 == 1 { v16 } else { v31 };
    let _ = v32;
    let v33: i128 = if v11 == 1 { v17 } else { v1 };
    let _ = v33;
    let v34: i128 = if v9 == 1 { v17 } else { v33 };
    let _ = v34;
    let v35: i128 = if v7 == 1 { v15 } else { v34 };
    let _ = v35;
    let v36: i128 = if v29 == 1 { v32 } else { v0 };
    let _ = v36;
    let v37: i128 = if v29 == 1 { v35 } else { v1 };
    let _ = v37;
    let v38: i128 = i128::from(v29 == 1 && v11 == 1);
    let _ = v38;
    let v39: i128 = input[4];
    let _ = v39;
    let v40: i128 = 4_i128;
    let _ = v40;
    let v41: i128 = i128::from(v39 == 0);
    let _ = v41;
    let v42: i128 = if v41 == 1 { v40 } else { v28 };
    let _ = v42;
    let v43: i128 = if v41 == 1 { v0 } else { v36 };
    let _ = v43;
    let v44: i128 = if v41 == 1 { v1 } else { v37 };
    let _ = v44;
    let v45: i128 = i128::from(v39 == 1 && v38 == 1);
    let _ = v45;
    if !(0_i128..=4_i128).contains(&v42) { return None; }
    if !(0_i128..=5_i128).contains(&v43) { return None; }
    if !(0_i128..=5_i128).contains(&v44) { return None; }
    if !(0_i128..=1_i128).contains(&v45) { return None; }
    Some([v42,v43,v44,v45,])
}
