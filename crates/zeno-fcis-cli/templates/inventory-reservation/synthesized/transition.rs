// Generated from zeno-fcis/finite-i64/1. Pure code; no commit authority.
#[rustfmt::skip]
pub fn transition(input: &[i64]) -> Option<[i64; 4]> {
    if input.len() != 4 { return None; }
    if !(0_i64..=5_i64).contains(&input[0]) { return None; }
    if !(0_i64..=5_i64).contains(&input[1]) { return None; }
    if !(0_i64..=3_i64).contains(&input[2]) { return None; }
    if !(1_i64..=3_i64).contains(&input[3]) { return None; }
    let v0: i64 = input[0];
    let _ = v0;
    let v1: i64 = input[1];
    let _ = v1;
    let v2: i64 = input[2];
    let _ = v2;
    let v3: i64 = input[3];
    let _ = v3;
    let v4: i64 = 5_i64;
    let _ = v4;
    let v5: i64 = 1_i64;
    let _ = v5;
    let v6: i64 = 0_i64;
    let _ = v6;
    let v7: i64 = i64::from(v2 == v6);
    let _ = v7;
    let v8: i64 = 1_i64;
    let _ = v8;
    let v9: i64 = i64::from(v2 == v8);
    let _ = v9;
    let v10: i64 = 2_i64;
    let _ = v10;
    let v11: i64 = i64::from(v2 == v10);
    let _ = v11;
    let v12: i64 = i64::from(v0 < v3);
    let _ = v12;
    let v13: i64 = i64::from(v1 < v3);
    let _ = v13;
    let v14: i64 = v0.checked_add(v3)?;
    let _ = v14;
    let v15: i64 = v1.checked_add(v3)?;
    let _ = v15;
    let v16: i64 = v0.checked_sub(v3)?;
    let _ = v16;
    let v17: i64 = v1.checked_sub(v3)?;
    let _ = v17;
    let v18: i64 = i64::from(v4 < v15);
    let _ = v18;
    let v19: i64 = i64::from(v4 < v14);
    let _ = v19;
    let v20: i64 = 3_i64;
    let _ = v20;
    let v21: i64 = if v18 == 1 { v10 } else { v20 };
    let _ = v21;
    let v22: i64 = if v12 == 1 { v6 } else { v21 };
    let _ = v22;
    let v23: i64 = if v19 == 1 { v10 } else { v20 };
    let _ = v23;
    let v24: i64 = if v13 == 1 { v5 } else { v23 };
    let _ = v24;
    let v25: i64 = if v13 == 1 { v5 } else { v20 };
    let _ = v25;
    let v26: i64 = if v11 == 1 { v25 } else { v23 };
    let _ = v26;
    let v27: i64 = if v9 == 1 { v24 } else { v26 };
    let _ = v27;
    let v28: i64 = if v7 == 1 { v22 } else { v27 };
    let _ = v28;
    let v29: i64 = i64::from(v28 == v20);
    let _ = v29;
    let v30: i64 = if v11 == 1 { v0 } else { v14 };
    let _ = v30;
    let v31: i64 = if v9 == 1 { v14 } else { v30 };
    let _ = v31;
    let v32: i64 = if v7 == 1 { v16 } else { v31 };
    let _ = v32;
    let v33: i64 = if v11 == 1 { v17 } else { v1 };
    let _ = v33;
    let v34: i64 = if v9 == 1 { v17 } else { v33 };
    let _ = v34;
    let v35: i64 = if v7 == 1 { v15 } else { v34 };
    let _ = v35;
    let v36: i64 = if v29 == 1 { v32 } else { v0 };
    let _ = v36;
    let v37: i64 = if v29 == 1 { v35 } else { v1 };
    let _ = v37;
    let v38: i64 = i64::from(v29 == 1 && v11 == 1);
    let _ = v38;
    if !(0_i64..=3_i64).contains(&v28) { return None; }
    if !(0_i64..=5_i64).contains(&v36) { return None; }
    if !(0_i64..=5_i64).contains(&v37) { return None; }
    if !(0_i64..=1_i64).contains(&v38) { return None; }
    Some([v28,v36,v37,v38,])
}
