// Generated from zeno-fcis/finite-i128/1. Pure code; no commit authority.
/// Evaluates the generated finite program; no commit authority.
#[rustfmt::skip]
pub fn transition(input: &[i128]) -> Option<[i128; 6]> {
    if input.len() != 8 { return None; }
    if !(0_i128..=1_i128).contains(&input[0]) { return None; }
    if !(0_i128..=1_i128).contains(&input[1]) { return None; }
    if !(0_i128..=2_i128).contains(&input[2]) { return None; }
    if !(0_i128..=1_i128).contains(&input[3]) { return None; }
    if !(0_i128..=1_i128).contains(&input[4]) { return None; }
    if !(0_i128..=1_i128).contains(&input[5]) { return None; }
    if !(0_i128..=1_i128).contains(&input[6]) { return None; }
    if !(0_i128..=1_i128).contains(&input[7]) { return None; }
    let v0: i128 = input[0];
    let _ = v0;
    let v1: i128 = input[1];
    let _ = v1;
    let v2: i128 = input[2];
    let _ = v2;
    let v3: i128 = input[3];
    let _ = v3;
    let v4: i128 = input[4];
    let _ = v4;
    let v5: i128 = input[5];
    let _ = v5;
    let v6: i128 = input[6];
    let _ = v6;
    let v7: i128 = input[7];
    let _ = v7;
    let v8: i128 = i128::from(v0 == 0);
    let _ = v8;
    let v9: i128 = i128::from(v5 == 0);
    let _ = v9;
    let v10: i128 = i128::from(v8 == 1 && v9 == 1);
    let _ = v10;
    let v11: i128 = i128::from(v10 == 0);
    let _ = v11;
    let v12: i128 = i128::from(v1 == 0);
    let _ = v12;
    let v13: i128 = i128::from(v6 == 0);
    let _ = v13;
    let v14: i128 = i128::from(v12 == 1 && v13 == 1);
    let _ = v14;
    let v15: i128 = i128::from(v14 == 0);
    let _ = v15;
    let v16: i128 = i128::from(v11 == 0);
    let _ = v16;
    let v17: i128 = i128::from(v15 == 0);
    let _ = v17;
    let v18: i128 = i128::from(v16 == 1 && v17 == 1);
    let _ = v18;
    let v19: i128 = i128::from(v18 == 0);
    let _ = v19;
    let v20: i128 = 0_i128;
    let _ = v20;
    let v21: i128 = i128::from(v20 < v2);
    let _ = v21;
    let v22: i128 = 1_i128;
    let _ = v22;
    let v23: i128 = i128::from(v2 == v22);
    let _ = v23;
    let v24: i128 = 1_i128;
    let _ = v24;
    let v25: i128 = v2.checked_sub(v24)?;
    let _ = v25;
    let v26: i128 = i128::from(v3 == 0);
    let _ = v26;
    let v27: i128 = i128::from(v7 == 1 && v26 == 1);
    let _ = v27;
    let v28: i128 = i128::from(v21 == 0);
    let _ = v28;
    let v29: i128 = i128::from(v28 == 1 && v27 == 1);
    let _ = v29;
    let v30: i128 = i128::from(v21 == 0);
    let _ = v30;
    let v31: i128 = i128::from(v29 == 0);
    let _ = v31;
    let v32: i128 = i128::from(v30 == 1 && v31 == 1);
    let _ = v32;
    let v33: i128 = 2_i128;
    let _ = v33;
    let v34: i128 = 1_i128;
    let _ = v34;
    let v35: i128 = if v4 == 1 { v33 } else { v34 };
    let _ = v35;
    let v36: i128 = 1_i128;
    let _ = v36;
    let v37: i128 = 2_i128;
    let _ = v37;
    let v38: i128 = 0_i128;
    let _ = v38;
    let v39: i128 = if v15 == 1 { v37 } else { v38 };
    let _ = v39;
    let v40: i128 = if v11 == 1 { v36 } else { v39 };
    let _ = v40;
    let v41: i128 = i128::from(v11 == 1 && v15 == 1);
    let _ = v41;
    let v42: i128 = if v41 == 1 { v35 } else { v40 };
    let _ = v42;
    let v43: i128 = 0_i128;
    let _ = v43;
    let v44: i128 = if v32 == 1 { v42 } else { v43 };
    let _ = v44;
    let v45: i128 = 0_i128;
    let _ = v45;
    let v46: i128 = i128::from(v44 == v45);
    let _ = v46;
    let v47: i128 = 1_i128;
    let _ = v47;
    let v48: i128 = i128::from(v44 == v47);
    let _ = v48;
    let v49: i128 = 2_i128;
    let _ = v49;
    let v50: i128 = i128::from(v44 == v49);
    let _ = v50;
    let v51: i128 = i128::from(v48 == 0);
    let _ = v51;
    let v52: i128 = i128::from(v11 == 1 && v51 == 1);
    let _ = v52;
    let v53: i128 = i128::from(v50 == 0);
    let _ = v53;
    let v54: i128 = i128::from(v15 == 1 && v53 == 1);
    let _ = v54;
    let v55: i128 = 2_i128;
    let _ = v55;
    let v56: i128 = 0_i128;
    let _ = v56;
    let v57: i128 = if v29 == 1 { v55 } else { v56 };
    let _ = v57;
    let v58: i128 = if v21 == 1 { v25 } else { v57 };
    let _ = v58;
    let v59: i128 = i128::from(v23 == 1 && v19 == 1);
    let _ = v59;
    let v60: i128 = 0_i128;
    let _ = v60;
    let v61: i128 = i128::from(v46 == 1 && v19 == 1);
    let _ = v61;
    let v62: i128 = i128::from(v3 == 1 && v61 == 1);
    let _ = v62;
    let v63: i128 = if v29 == 1 { v60 } else { v62 };
    let _ = v63;
    let v64: i128 = if v21 == 1 { v59 } else { v63 };
    let _ = v64;
    let v65: i128 = 1_i128;
    let _ = v65;
    let v66: i128 = 0_i128;
    let _ = v66;
    let v67: i128 = if v50 == 1 { v66 } else { v4 };
    let _ = v67;
    let v68: i128 = if v48 == 1 { v65 } else { v67 };
    let _ = v68;
    if !(0_i128..=2_i128).contains(&v44) { return None; }
    if !(0_i128..=1_i128).contains(&v52) { return None; }
    if !(0_i128..=1_i128).contains(&v54) { return None; }
    if !(0_i128..=2_i128).contains(&v58) { return None; }
    if !(0_i128..=1_i128).contains(&v64) { return None; }
    if !(0_i128..=1_i128).contains(&v68) { return None; }
    Some([v44,v52,v54,v58,v64,v68,])
}
