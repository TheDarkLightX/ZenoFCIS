// Generated from zeno-fcis/finite-i128/1. Pure code; no commit authority.
/// Evaluates the generated finite program; no commit authority.
#[rustfmt::skip]
pub fn transition(input: &[i128]) -> Option<[i128; 1]> {
    if input.len() != 12 { return None; }
    if !(0_i128..=2_i128).contains(&input[0]) { return None; }
    if !(0_i128..=1_i128).contains(&input[1]) { return None; }
    if !(0_i128..=1_i128).contains(&input[2]) { return None; }
    if !(0_i128..=1_i128).contains(&input[3]) { return None; }
    if !(0_i128..=1_i128).contains(&input[4]) { return None; }
    if !(0_i128..=1_i128).contains(&input[5]) { return None; }
    if !(0_i128..=1_i128).contains(&input[6]) { return None; }
    if !(0_i128..=1_i128).contains(&input[7]) { return None; }
    if !(0_i128..=1_i128).contains(&input[8]) { return None; }
    if !(0_i128..=1_i128).contains(&input[9]) { return None; }
    if !(0_i128..=1_i128).contains(&input[10]) { return None; }
    if !(0_i128..=1_i128).contains(&input[11]) { return None; }
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
    let v8: i128 = input[8];
    let _ = v8;
    let v9: i128 = input[9];
    let _ = v9;
    let v10: i128 = input[10];
    let _ = v10;
    let v11: i128 = input[11];
    let _ = v11;
    let v12: i128 = 3_i128;
    let _ = v12;
    let v13: i128 = 4_i128;
    let _ = v13;
    let v14: i128 = 9_i128;
    let _ = v14;
    let v15: i128 = 10_i128;
    let _ = v15;
    let v16: i128 = 11_i128;
    let _ = v16;
    let v17: i128 = 12_i128;
    let _ = v17;
    let v18: i128 = 1_i128;
    let _ = v18;
    let v19: i128 = if v18 == 1 { v3 } else { v4 };
    let _ = v19;
    let v20: i128 = if v18 == 1 { v12 } else { v13 };
    let _ = v20;
    let v21: i128 = if v18 == 1 { v4 } else { v3 };
    let _ = v21;
    let v22: i128 = if v18 == 1 { v13 } else { v12 };
    let _ = v22;
    let v23: i128 = 1_i128;
    let _ = v23;
    let v24: i128 = if v23 == 1 { v8 } else { v9 };
    let _ = v24;
    let v25: i128 = if v23 == 1 { v14 } else { v15 };
    let _ = v25;
    let v26: i128 = if v23 == 1 { v9 } else { v8 };
    let _ = v26;
    let v27: i128 = if v23 == 1 { v15 } else { v14 };
    let _ = v27;
    let v28: i128 = 1_i128;
    let _ = v28;
    let v29: i128 = if v28 == 1 { v10 } else { v11 };
    let _ = v29;
    let v30: i128 = if v28 == 1 { v16 } else { v17 };
    let _ = v30;
    let v31: i128 = if v28 == 1 { v11 } else { v10 };
    let _ = v31;
    let v32: i128 = if v28 == 1 { v17 } else { v16 };
    let _ = v32;
    let v33: i128 = 5_i128;
    let _ = v33;
    let v34: i128 = 0_i128;
    let _ = v34;
    let v35: i128 = i128::from(v31 == 0);
    let _ = v35;
    let v36: i128 = if v35 == 1 { v32 } else { v34 };
    let _ = v36;
    let v37: i128 = i128::from(v29 == 0);
    let _ = v37;
    let v38: i128 = if v37 == 1 { v30 } else { v36 };
    let _ = v38;
    let v39: i128 = i128::from(v26 == 0);
    let _ = v39;
    let v40: i128 = if v39 == 1 { v27 } else { v38 };
    let _ = v40;
    let v41: i128 = i128::from(v24 == 0);
    let _ = v41;
    let v42: i128 = if v41 == 1 { v25 } else { v40 };
    let _ = v42;
    let v43: i128 = if v5 == 1 { v33 } else { v42 };
    let _ = v43;
    let v44: i128 = i128::from(v21 == 0);
    let _ = v44;
    let v45: i128 = if v44 == 1 { v22 } else { v43 };
    let _ = v45;
    let v46: i128 = i128::from(v19 == 0);
    let _ = v46;
    let v47: i128 = if v46 == 1 { v20 } else { v45 };
    let _ = v47;
    let v48: i128 = 6_i128;
    let _ = v48;
    let v49: i128 = 7_i128;
    let _ = v49;
    let v50: i128 = 8_i128;
    let _ = v50;
    let v51: i128 = i128::from(v7 == 0);
    let _ = v51;
    let v52: i128 = if v51 == 1 { v50 } else { v34 };
    let _ = v52;
    let v53: i128 = i128::from(v6 == 0);
    let _ = v53;
    let v54: i128 = if v53 == 1 { v49 } else { v52 };
    let _ = v54;
    let v55: i128 = i128::from(v5 == 0);
    let _ = v55;
    let v56: i128 = if v55 == 1 { v48 } else { v54 };
    let _ = v56;
    let v57: i128 = 13_i128;
    let _ = v57;
    let v58: i128 = if v53 == 1 { v49 } else { v57 };
    let _ = v58;
    let v59: i128 = if v55 == 1 { v48 } else { v58 };
    let _ = v59;
    let v60: i128 = i128::from(v0 == v34);
    let _ = v60;
    let v61: i128 = 1_i128;
    let _ = v61;
    let v62: i128 = i128::from(v0 == v61);
    let _ = v62;
    let v63: i128 = if v62 == 1 { v56 } else { v59 };
    let _ = v63;
    let v64: i128 = if v60 == 1 { v47 } else { v63 };
    let _ = v64;
    let v65: i128 = 2_i128;
    let _ = v65;
    let v66: i128 = i128::from(v2 == 0);
    let _ = v66;
    let v67: i128 = if v66 == 1 { v65 } else { v64 };
    let _ = v67;
    let v68: i128 = i128::from(v1 == 0);
    let _ = v68;
    let v69: i128 = if v68 == 1 { v61 } else { v67 };
    let _ = v69;
    if !(0_i128..=13_i128).contains(&v69) { return None; }
    Some([v69,])
}
