#![allow(dead_code, unreachable_pub)]
pub fn hex(s: &str) -> Vec<u8> {
    let s = s.strip_prefix("0x").unwrap();
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap())
        .collect()
}
pub fn list(s: &str) -> Vec<Vec<u8>> {
    if s == "-" {
        vec![]
    } else {
        s.split(',').map(hex).collect()
    }
}
#[derive(Debug)]
pub struct Vector {
    pub name: &'static str,
    pub op: &'static str,
    pub dst: Vec<u8>,
    pub a: Vec<Vec<u8>>,
    pub b: Vec<Vec<u8>>,
    pub c: Vec<Vec<u8>>,
    pub expected: &'static str,
}
pub fn vectors() -> Vec<Vector> {
    include_str!("../test-data/vectors.tsv")
        .lines()
        .map(|l| {
            let f = l.split('\t').collect::<Vec<_>>();
            assert_eq!(f.len(), 7);
            Vector {
                name: f[0],
                op: f[1],
                dst: hex(f[2]),
                a: list(f[3]),
                b: list(f[4]),
                c: list(f[5]),
                expected: f[6],
            }
        })
        .collect()
}
pub fn named(name: &str) -> Vector {
    vectors().into_iter().find(|v| v.name == name).unwrap()
}
