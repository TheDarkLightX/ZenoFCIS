//! Native oracle using the same library call paths as the browser module.
use std::io::{self, BufRead};
fn main() {
    for line in io::stdin().lock().lines() {
        let line = line.unwrap_or_else(|e| panic!("stdin: {e}"));
        let request = serde_json::from_str(&line).unwrap_or_else(|e| panic!("JSON: {e}"));
        let response =
            zeno_fcis_site_factory::run(&request).unwrap_or_else(|e| panic!("checker: {e}"));
        println!("{response}");
    }
}
