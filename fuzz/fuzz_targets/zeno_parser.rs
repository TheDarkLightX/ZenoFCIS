#![no_main]

use libfuzzer_sys::fuzz_target;
use zeno_fcis_spec::{ProjectLimits, SourceLimits, elaborate_project, parse_project};

fuzz_target!(|input: &[u8]| {
    let Ok(source) = core::str::from_utf8(input) else {
        return;
    };
    if let Ok(parsed) = parse_project(source, SourceLimits::default()) {
        let _ = elaborate_project(parsed, ProjectLimits::default());
    }
});
