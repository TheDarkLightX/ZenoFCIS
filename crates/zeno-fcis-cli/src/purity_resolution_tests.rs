//! Independent source specimens for the actual resolved-use route.

use super::{Report, check_resolved_paths};
use std::collections::BTreeSet;
use std::fs;
use std::io;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT: AtomicU64 = AtomicU64::new(0);

struct Fixture(PathBuf);
impl Fixture {
    fn new(files: &[(&str, &str)]) -> io::Result<Self> {
        let root = std::env::temp_dir().join(format!(
            "zeno-resolved-purity-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::SeqCst)
        ));
        fs::create_dir(&root)?;
        let fixture = Self(root);
        for (path, source) in files {
            let target = fixture.0.join(path);
            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::write(target, source)?;
        }
        Ok(fixture)
    }
    fn source(source: &str) -> io::Result<Self> {
        Self::new(&[("input.rs", source)])
    }
    fn report(&self) -> Report {
        let path = if self.0.join("Cargo.toml").is_file() {
            self.0.clone()
        } else {
            self.0.join("input.rs")
        };
        check_resolved_paths(&[path])
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

const MANIFEST: &str =
    "[package]\nname='specimen'\nversion='0.0.0'\nedition='2024'\n[dependencies]\n";
const HEAD: &str = "#![no_std]\n#![forbid(unsafe_code)]\n";

fn rules(report: &Report) -> BTreeSet<&'static str> {
    report.findings.iter().map(|f| f.rule).collect()
}

#[test]
fn deterministic_scopes_and_builtin_arguments_are_accepted() -> io::Result<()> {
    let specimens = [
        "use alloc::{collections::BTreeMap, vec::Vec}; fn f(x:u64)->Vec<u64> { let mut map=BTreeMap::new(); map.insert(x,x); vec![x, x+1] }",
        "mod std { pub mod time { pub fn now()->u64{0} } } fn f()->u64 { std::time::now() }",
        "fn f(std:u64)->u64 { let std=std+1; let std=std+1; std }",
        "mod left { use alloc::vec::Vec as Store; pub fn f()->Store<u8>{Store::new()} } mod right { pub struct Store; pub fn f()->Store{Store} }",
        "use alloc::vec::Vec as A; use A as B; type C<T> = B<T>; fn f()->C<u64>{C::new()}",
        "struct Item<T>{value:T} impl<T> Item<T> { fn get(&self)->&T{&self.value} }",
        "fn f(x:Option<u64>)->u64{if let Some(value)=x {value} else {0}}",
        "fn f(x:Option<u64>)->bool{matches!(x, Some(value) if value>3)}",
        "fn f(x:u64)->alloc::string::String { format!(\"{}\", x) }",
        "#[cfg(test)] mod tests { fn f(){std::time::Instant::now();} } fn f()->u8{0}",
        "#[cfg(verus_keep_ghost)] mod proof { verus! { unsupported spec tokens @ } } fn f()->u8{0}",
        "#[cfg(not(not(test)))] fn inactive(){std::fs::read(\"x\");} fn f()->u8{0}",
        "#![no_std] fn f(x:u8)->alloc::vec::Vec<u8>{vec![{let y=x+1;y}]}",
        "fn f(x:Option<u64>)->bool{if let Some(value)=x && value>3 {true}else{false}}",
        "struct V<const N:usize>{data:[u8;N]} impl<const N:usize> V<N>{fn f()->Self{Self{data:[0;N]}}}",
        "fn f(x:u64)->bool { #[cfg(verus_keep_ghost)] proof!{spec tokens @} x>0 }",
    ];
    for source in specimens {
        let fixture = Fixture::source(source)?;
        let report = fixture.report();
        assert_eq!(report.errors(), 0, "{source}\n{report:?}");
        assert!(report.unreadable.is_empty(), "{source}\n{report:?}");
        assert!(report.resolution.is_some());
    }
    Ok(())
}

#[test]
fn alias_paths_reach_each_ambient_rule() -> io::Result<()> {
    let cases = [
        (
            "use std::{time::{Instant as Clock}}; fn f(){ Clock::now(); }",
            "clock",
        ),
        (
            "extern crate std as host; use host::env as e; use e::var as fetch; fn f(){fetch(\"SECRET\");}",
            "environment",
        ),
        (
            "use rand as r; use r::random as sample; fn f(){ sample::<u64>(); }",
            "randomness",
        ),
        (
            "type Store<K,V> = alloc::collections::HashMap<K,V>; fn f(){let _=Store::<u8,u8>::new();}",
            "hash-order",
        ),
        (
            "use core::sync::{atomic::{AtomicU64 as Counter}}; fn f(){Counter::new(0);}",
            "shared-state",
        ),
        (
            "use core::{ptr as address}; fn f(){address::null::<u8>();}",
            "address",
        ),
        (
            "mod bridge { pub use std::time::Instant as Clock; } use crate::bridge::Clock as Now; fn f(){Now::now();}",
            "clock",
        ),
        (
            "use std as platform; use platform::{fs as files}; fn f(){files::read(\"x\");}",
            "filesystem",
        ),
    ];
    for (source, rule) in cases {
        let report = Fixture::source(source)?.report();
        assert!(rules(&report).contains(rule), "{source}\n{report:?}");
        assert_eq!(report.status(), "violations");
    }
    Ok(())
}

#[test]
fn lexical_shadow_cannot_erase_another_scopes_effect() -> io::Result<()> {
    let source = "use std::time::Instant as Clock; mod harmless{pub struct Clock; pub fn f()->Clock{Clock}} fn f(){Clock::now();} fn g(Clock:u64)->u64{Clock}";
    let report = Fixture::source(source)?.report();
    assert!(rules(&report).contains("clock"), "{report:?}");
    assert!(
        !rules(&report).contains("resolution-ambiguity"),
        "{report:?}"
    );
    Ok(())
}

#[test]
fn declared_external_modules_and_parent_paths_are_followed() -> io::Result<()> {
    let fixture = Fixture::new(&[
        ("Cargo.toml", MANIFEST),
        (
            "src/lib.rs",
            &format!("{HEAD}mod outer; pub use outer::Clock as Time; fn f(){{Time::now();}}"),
        ),
        (
            "src/outer/mod.rs",
            "pub use std::time::Instant as Clock; mod nested;",
        ),
        (
            "src/outer/nested.rs",
            "use super::Clock as Parent; fn f(){Parent::now();}",
        ),
        (
            "src/unreachable.rs",
            "fn f(){std::net::TcpStream::connect(\"x\");}",
        ),
    ])?;
    let report = fixture.report();
    assert!(rules(&report).contains("clock"), "{report:?}");
    assert!(!rules(&report).contains("network"), "{report:?}");
    let json = report.to_json();
    assert_eq!(
        json["resolution"]["active_files"].as_array().map(Vec::len),
        Some(3)
    );
    Ok(())
}

#[test]
fn globs_are_resolved_only_with_a_declared_export_set() -> io::Result<()> {
    let positive =
        Fixture::source("mod values {pub fn zero()->u64{0}} use values::*; fn f()->u64{zero()}")?
            .report();
    assert_eq!(positive.errors(), 0, "{positive:?}");
    let ambiguous = Fixture::source(
        "mod a{pub fn x(){}} mod b{pub fn x(){}} use a::*; use b::*; fn f(){x();}",
    )?
    .report();
    assert!(
        rules(&ambiguous).contains("resolution-ambiguity"),
        "{ambiguous:?}"
    );
    let external = Fixture::source("use std::time::*; fn f(){Instant::now();}")?.report();
    assert!(rules(&external).contains("clock"), "{external:?}");
    assert!(rules(&external).contains("resolution-glob"), "{external:?}");
    Ok(())
}

#[test]
fn cycles_unknown_roots_and_missing_modules_refuse() -> io::Result<()> {
    for (source, expected) in [
        (
            "use B as A; use A as B; fn f(){A::call();}",
            "resolution-cycle",
        ),
        ("fn f(){unknown::effect();}", "resolution-unresolved"),
        ("mod absent; fn f(){}", "resolution-module"),
        (
            "#[path=\"elsewhere.rs\"] mod elsewhere;",
            "resolution-module-path",
        ),
        ("#[unknown_expansion] fn f(){}", "resolution-attribute"),
    ] {
        let report = Fixture::source(source)?.report();
        assert!(rules(&report).contains(expected), "{source}\n{report:?}");
        assert_eq!(report.status(), "violations");
    }
    let malformed = Fixture::source("fn broken( {")?.report();
    assert_eq!(malformed.status(), "unreadable");
    Ok(())
}

#[test]
fn macro_arguments_use_their_actual_scopes_and_unknown_expansions_refuse() -> io::Result<()> {
    let cases = [
        (
            "use std::time::Instant as Clock; fn f(){let _=vec![Clock::now()];}",
            "clock",
        ),
        (
            "use std::env::var as fetch; fn f(){assert!(fetch(\"x\").is_ok());}",
            "environment",
        ),
        ("fn f(){let _=env!(\"HOME\");}", "environment"),
        ("fn f(){let _=format!(\"{:p}\", &0);}", "address"),
        (
            "macro_rules! pure {()=>{std::time::Instant::now()}} fn f(){pure!();}",
            "resolution-macro",
        ),
        (
            "macro_rules! vec {()=>{std::time::Instant::now()}} fn f(){vec!();}",
            "resolution-macro",
        ),
        ("include!(\"outside.rs\");", "resolution-macro"),
        ("#[derive(Custom)] struct X;", "resolution-macro"),
        (
            "mod custom{#[macro_export] macro_rules! vec{()=>{std::time::Instant::now()}}} fn f(){vec!();}",
            "resolution-macro",
        ),
        (
            "#[macro_use] mod custom{macro_rules! vec{()=>{std::time::Instant::now()}}} fn f(){vec!();}",
            "resolution-attribute",
        ),
        (
            "#[macro_use] extern crate alloc; fn f(){vec![0];}",
            "resolution-attribute",
        ),
    ];
    for (source, expected) in cases {
        let report = Fixture::source(source)?.report();
        assert!(rules(&report).contains(expected), "{source}\n{report:?}");
    }
    Ok(())
}

#[test]
fn intrinsic_unsafe_address_async_and_float_rules_are_retained() -> io::Result<()> {
    for (source, expected) in [
        ("unsafe fn f(){}", "unsafe"),
        ("fn f(){unsafe{}}", "unsafe"),
        ("unsafe extern \"C\"{fn f();}", "foreign-code"),
        ("static mut STATE:u64=0;", "shared-state"),
        ("fn f(p:*const u8)->usize{p.addr()}", "address"),
        ("async fn f(){}", "threads"),
        ("fn f()->f64{1.5}", "floating-point"),
    ] {
        let report = Fixture::source(source)?.report();
        assert!(rules(&report).contains(expected), "{source}\n{report:?}");
    }
    Ok(())
}

#[test]
fn unresolved_manifest_identity_cannot_be_clean() -> io::Result<()> {
    let fixture = Fixture::new(&[
        (
            "Cargo.toml",
            "[package]\nname='specimen'\nversion='0.0.0'\n[dependencies]\nplatform={package='std',version='=0.0.0'}\n",
        ),
        (
            "src/lib.rs",
            &format!("{HEAD}use platform::time::Instant as Clock; fn f(){{Clock::now();}}"),
        ),
    ])?;
    let report = fixture.report();
    assert!(rules(&report).contains("resolution-manifest"), "{report:?}");
    assert_eq!(report.status(), "violations");
    Ok(())
}

#[test]
fn original_source_target_diagnostics_survive_the_resolved_route() -> io::Result<()> {
    let binary = Fixture::new(&[
        ("Cargo.toml", MANIFEST),
        ("src/lib.rs", HEAD),
        ("src/bin/hidden.rs", "fn main(){std::time::Instant::now();}"),
    ])?;
    let report = binary.report();
    assert!(rules(&report).contains("resolution-target"), "{report:?}");
    assert!(
        report.structures.iter().any(|s| s
            .binary_targets
            .iter()
            .any(|p| p == &binary.0.join("src/bin").display().to_string())),
        "{report:?}"
    );
    let include = Fixture::new(&[
        ("Cargo.toml", MANIFEST),
        ("src/lib.rs", &format!("{HEAD}include!(\"outside.rs\");")),
    ])?;
    let report = include.report();
    assert!(rules(&report).contains("resolution-source"), "{report:?}");
    assert!(rules(&report).contains("resolution-macro"), "{report:?}");
    assert!(
        report
            .structures
            .iter()
            .any(|s| s.external_sources.iter().any(|p| p.contains("`include`"))),
        "{report:?}"
    );
    Ok(())
}

#[test]
fn feature_alternatives_and_macro_introduced_scopes_are_not_discarded() -> io::Result<()> {
    let report =
        Fixture::source("#[cfg(feature=\"optional\")] fn f(){std::time::Instant::now();}")?
            .report();
    assert!(rules(&report).contains("clock"), "{report:?}");
    let report = Fixture::source("fn f(){let _=vec![{std::env::var(\"x\") }];}")?.report();
    assert!(rules(&report).contains("environment"), "{report:?}");
    assert_eq!(report.status(), "violations");
    let report = Fixture::source(
        "mod custom{pub use effect::Debug;} use custom::Debug; #[derive(Debug)] struct X;",
    )?
    .report();
    assert!(report.errors() > 0, "{report:?}");
    Ok(())
}

#[test]
fn multiple_selected_crates_follow_only_declared_dependencies() -> io::Result<()> {
    let fixture = Fixture::new(&[
        (
            "one/Cargo.toml",
            "[package]\nname='one'\nversion='0.0.0'\n[dependencies]\nzeno-fcis-core='=0.0.0'\n",
        ),
        (
            "one/src/lib.rs",
            &format!("{HEAD}use zeno_fcis_core::Clock as Time; fn f(){{Time::now();}}"),
        ),
        (
            "two/Cargo.toml",
            "[package]\nname='zeno-fcis-core'\nversion='0.0.0'\n[dependencies]\n",
        ),
        (
            "two/src/lib.rs",
            &format!("{HEAD}pub use std::time::Instant as Clock;"),
        ),
    ])?;
    let report = check_resolved_paths(&[fixture.0.join("one"), fixture.0.join("two")]);
    assert!(rules(&report).contains("clock"), "{report:?}");
    assert!(
        !rules(&report).contains("resolution-unresolved"),
        "{report:?}"
    );
    Ok(())
}

#[test]
fn external_module_inner_attributes_and_cfg_determine_native_custody() -> io::Result<()> {
    let inactive = Fixture::new(&[
        ("Cargo.toml", MANIFEST),
        ("src/lib.rs", &format!("{HEAD}mod ghost; fn f()->u8{{0}}")),
        (
            "src/ghost.rs",
            "#![cfg(verus_keep_ghost)]\nverus!{ unsupported proof syntax @ }",
        ),
    ])?;
    let report = inactive.report();
    assert_eq!(report.errors(), 0, "{report:?}");
    assert_eq!(
        report.to_json()["resolution"]["active_files"]
            .as_array()
            .map(Vec::len),
        Some(1)
    );
    let expansion = Fixture::new(&[
        ("Cargo.toml", MANIFEST),
        ("src/lib.rs", &format!("{HEAD}mod expanded;")),
        ("src/expanded.rs", "#![custom_expansion]\nfn f()->u8{0}"),
    ])?;
    let report = expansion.report();
    assert!(
        rules(&report).contains("resolution-attribute"),
        "{report:?}"
    );
    Ok(())
}

#[test]
fn inherited_builtin_named_user_macros_refuse_in_textual_scopes() -> io::Result<()> {
    // The first two strings are the independent review's unchanged specimens.
    for source in [
        "macro_rules! stringify { () => { 7u8 }; }\npub fn value()->u8 { stringify!() }\n",
        "macro_rules! stringify { () => { 7u8 }; }\npub mod child { pub fn value()->u8 { stringify!() } }\n",
        "macro_rules! stringify { () => { 7u8 }; } mod outer { mod inner { pub fn value()->u8 { stringify!() } } }",
        "pub fn value()->u8 { macro_rules! stringify { () => { 7u8 }; } { stringify!() } }",
        "macro_rules! stringify { () => { 7u8 }; } pub fn value()->u8 { (|| stringify!())() }",
    ] {
        let report = Fixture::source(source)?.report();
        assert_eq!(report.status(), "violations", "{source}\n{report:?}");
        assert!(rules(&report).contains("resolution-macro"), "{report:?}");
    }
    let confined = Fixture::new(&[
        ("Cargo.toml", MANIFEST),
        (
            "src/lib.rs",
            "#![no_std]\n#![forbid(unsafe_code)]\nmacro_rules! stringify { () => { 7u8 }; }\npub mod child { pub fn value()->u8 { stringify!() } }\n",
        ),
    ])?;
    assert_eq!(confined.report().status(), "violations");
    Ok(())
}

#[test]
fn textual_macro_declaration_order_keeps_known_builtins() -> io::Result<()> {
    for source in [
        "pub fn value()->&'static str { stringify!(seven) } macro_rules! stringify { () => { 7u8 }; }",
        "mod child { pub fn value()->&'static str { stringify!(seven) } } macro_rules! stringify { () => { 7u8 }; }",
        "macro_rules! stringify { () => { 7u8 }; } pub fn value()->&'static str { core::stringify!(seven) }",
        "use core::stringify as show; pub fn value()->&'static str { show!(seven) } macro_rules! show { () => { 7u8 }; }",
        "#[cfg(test)] macro_rules! stringify { () => { 7u8 }; } pub fn value()->&'static str { stringify!(seven) }",
    ] {
        let report = Fixture::source(source)?.report();
        assert_eq!(report.errors(), 0, "{source}\n{report:?}");
        assert!(report.unreadable.is_empty(), "{source}\n{report:?}");
    }
    let report = Fixture::source(
        "pub fn early()->&'static str { stringify!(seven) } macro_rules! stringify { () => { 7u8 }; } pub fn late()->u8 { stringify!() }",
    )?
    .report();
    assert_eq!(report.findings.len(), 1, "{report:?}");
    assert!(rules(&report).contains("resolution-macro"));
    Ok(())
}

#[test]
fn external_macro_inheritance_uses_parent_module_declaration_position() -> io::Result<()> {
    for (root, child, refusal) in [
        (
            "macro_rules! stringify { () => { 7u8 }; }\npub mod child;\n",
            "pub fn value()->u8 { stringify!() }\n",
            true,
        ),
        (
            "pub mod child;\nmacro_rules! stringify { () => { 7u8 }; }\n",
            "\n\n\n\n\n\n\npub fn value()->&'static str { stringify!(seven) }\n",
            false,
        ),
    ] {
        let fixture = Fixture::new(&[
            ("Cargo.toml", MANIFEST),
            ("src/lib.rs", &format!("{HEAD}{root}")),
            ("src/child.rs", child),
        ])?;
        let report = fixture.report();
        assert_eq!(
            rules(&report).contains("resolution-macro"),
            refusal,
            "{report:?}"
        );
        assert_eq!(report.errors() > 0, refusal, "{report:?}");
        assert_eq!(
            report.to_json()["resolution"]["active_files"]
                .as_array()
                .map(Vec::len),
            Some(2)
        );
    }
    let nested = Fixture::new(&[
        ("Cargo.toml", MANIFEST),
        (
            "src/lib.rs",
            &format!("{HEAD}macro_rules! stringify {{ () => {{ 7u8 }}; }} mod outer;"),
        ),
        ("src/outer/mod.rs", "mod inner;"),
        ("src/outer/inner.rs", "pub fn value()->u8 { stringify!() }"),
    ])?;
    let report = nested.report();
    assert!(rules(&report).contains("resolution-macro"), "{report:?}");
    assert_eq!(
        report.to_json()["resolution"]["active_files"]
            .as_array()
            .map(Vec::len),
        Some(3)
    );
    Ok(())
}

#[test]
fn macro_export_and_import_do_not_authorize_user_expansion() -> io::Result<()> {
    for source in [
        "mod definitions { #[macro_export] macro_rules! stringify { () => { 7u8 }; } } mod child { use crate::stringify; pub fn value()->u8 { stringify!() } }",
        "macro_rules! local_value { () => { 7u8 }; } pub(crate) use local_value as stringify; mod child { use super::stringify; pub fn value()->u8 { stringify!() } }",
        "pub fn value()->u8 { crate::stringify!() } mod definitions { #[macro_export] macro_rules! stringify { () => { 7u8 }; } }",
    ] {
        let report = Fixture::source(source)?.report();
        assert!(rules(&report).contains("resolution-macro"), "{report:?}");
        assert_eq!(report.status(), "violations");
    }
    let report = Fixture::source(
        "mod builtin { pub use core::stringify as show; } use builtin::show; pub fn value()->&'static str { show!(seven) }",
    )?
    .report();
    assert_eq!(report.errors(), 0, "{report:?}");
    Ok(())
}

#[test]
fn macro_inheritance_does_not_widen_ordinary_namespace_lookup() -> io::Result<()> {
    for source in [
        "pub fn root_value()->u8{7} mod child { pub fn value()->u8 { root_value() } }",
        "pub struct RootType; mod child { pub fn value()->RootType { RootType } }",
    ] {
        let report = Fixture::source(source)?.report();
        assert!(
            rules(&report).contains("resolution-unresolved"),
            "{report:?}"
        );
    }
    let report = Fixture::source(
        "pub fn root_value()->u8{7} mod child { pub fn value()->u8 { super::root_value() } }",
    )?
    .report();
    assert_eq!(report.errors(), 0, "{report:?}");
    Ok(())
}

#[test]
fn package_without_established_roots_refuses_on_the_manifest() -> io::Result<()> {
    for files in [
        vec![
            (
                "Cargo.toml",
                "[package]\nname='custom_root_fixture'\nversion='0.0.0'\nedition='2024'\n[workspace]\n[lib]\npath='src/entry.rs'\n",
            ),
            (
                "src/entry.rs",
                "#![forbid(unsafe_code)]\nmacro_rules! local_value { () => { 7u8 }; }\npub fn value()->u8 { local_value!() }\n",
            ),
        ],
        vec![
            (
                "Cargo.toml",
                "[package]\nname='bin_root_fixture'\nversion='0.0.0'\nedition='2024'\n[workspace]\n",
            ),
            (
                "src/bin/worker.rs",
                "macro_rules! local_value { () => { 7u8 }; }\nfn main(){ let _ = local_value!(); }\n",
            ),
        ],
    ] {
        let fixture = Fixture::new(&files)?;
        let report = fixture.report();
        assert_eq!(report.status(), "violations", "{report:?}");
        assert!(rules(&report).contains("resolution-target"), "{report:?}");
        assert!(
            report.findings.iter().all(|finding| {
                finding.file == fixture.0.join("Cargo.toml").display().to_string()
            })
        );
        assert_eq!(
            report.to_json()["resolution"]["active_files"]
                .as_array()
                .map(Vec::len),
            Some(0)
        );
        assert!(
            report
                .structures
                .iter()
                .any(|s| { !s.unrecognized_manifest.is_empty() || !s.binary_targets.is_empty() })
        );
    }
    Ok(())
}

#[test]
fn established_inactive_root_is_distinct_from_unreadable_or_missing_root() -> io::Result<()> {
    let inactive = Fixture::new(&[
        ("Cargo.toml", MANIFEST),
        (
            "src/lib.rs",
            "#![cfg(test)]\n#![no_std]\n#![forbid(unsafe_code)]\nunsupported!();",
        ),
    ])?;
    let report = inactive.report();
    assert_eq!(report.errors(), 0, "{report:?}");
    assert!(report.unreadable.is_empty());
    assert!(
        report
            .resolution
            .as_ref()
            .is_some_and(|r| { r.active_files.is_empty() && !r.skipped_cfg.is_empty() })
    );
    let broken = Fixture::new(&[("Cargo.toml", MANIFEST), ("src/lib.rs", "fn broken( {")])?;
    let report = broken.report();
    assert_eq!(report.status(), "unreadable", "{report:?}");
    assert!(rules(&report).contains("resolution-target"));
    let unsupported = Fixture::new(&[
        (
            "Cargo.toml",
            "[package]\nname='specimen'\nversion='0.0.0'\nautolib=false\n",
        ),
        ("src/lib.rs", "#![cfg(test)]\n"),
    ])?;
    let report = unsupported.report();
    assert!(rules(&report).contains("resolution-manifest"), "{report:?}");
    assert_eq!(report.status(), "violations");
    Ok(())
}

#[test]
fn qualified_attribute_prefixes_are_not_builtin_attributes() -> io::Result<()> {
    for source in [
        "#![no_std]\n#![forbid(unsafe_code)]\n#[doc::noop]\npub fn value()->u8{7}\n",
        "#[allow::noop] pub fn value()->u8{7}",
        "#[repr::noop] pub fn value()->u8{7}",
        "#[rustfmt::noop] pub fn value()->u8{7}",
        "#[cfg_attr(not(test), doc::noop)] pub fn value()->u8{7}",
    ] {
        let report = Fixture::source(source)?.report();
        assert!(
            rules(&report).contains("resolution-attribute"),
            "{source}\n{report:?}"
        );
        assert_eq!(report.status(), "violations");
    }
    Ok(())
}

#[test]
fn exact_builtin_and_supported_tool_attributes_are_retained() -> io::Result<()> {
    for source in [
        "#![no_std] #![forbid(unsafe_code)] #[doc=\"state\"] #[allow(dead_code)] #[derive(Clone, Copy)] #[repr(C)] #[non_exhaustive] pub struct State { value:u8 } #[rustfmt::skip] pub fn value()->u8{7}",
        "#[cfg_attr(test, doc::noop)] pub fn value()->u8{7}",
        "#[cfg_attr(not(test), inline)] pub fn value()->u8{7}",
    ] {
        let report = Fixture::source(source)?.report();
        assert_eq!(report.errors(), 0, "{source}\n{report:?}");
        assert!(report.unreadable.is_empty());
    }
    Ok(())
}

#[test]
fn explicit_raw_const_and_mut_borrows_report_address_exposure() -> io::Result<()> {
    for source in [
        "pub fn address(x: &u8) -> usize { (&raw const *x) as usize }\n",
        "pub fn address(x: &mut u8) -> usize { (&raw mut *x) as usize }\n",
    ] {
        let report = Fixture::source(source)?.report();
        assert_eq!(report.status(), "violations", "{report:?}");
        assert!(
            report
                .findings
                .iter()
                .any(|f| { f.rule == "address" && f.subject == "raw borrow expression" })
        );
    }
    let report =
        Fixture::source("pub fn shared(x:&u8)->&u8{x} pub fn exclusive(x:&mut u8)->&mut u8{x}")?
            .report();
    assert_eq!(report.errors(), 0, "{report:?}");
    Ok(())
}

#[test]
fn generic_alias_targets_use_their_actual_parameter_scope() -> io::Result<()> {
    for source in [
        "type Identity<T> = T; pub fn value()->Identity<u8> { 7 }\n",
        "type Identity<std> = std; pub fn value()->Identity<u8> { 7 }",
        "type Identity<T> = T; type Second<T> = Identity<T>; pub fn value()->Second<u8> { 7 }",
        "type Array<const N:usize> = [u8;N]; pub fn value()->Array<1>{[7]}",
        "extern crate alloc; mod first{pub type Identity<T>=T;} mod second{type Identity<T>=alloc::vec::Vec<T>; pub fn value()->Identity<u8>{Identity::new()}} pub fn value()->first::Identity<u8>{7}",
    ] {
        let report = Fixture::source(source)?.report();
        assert_eq!(report.errors(), 0, "{source}\n{report:?}");
        assert!(report.unreadable.is_empty());
    }
    for (source, rule) in [
        (
            "type Identity<T> = T; pub fn value(x:Identity<std::time::Instant>){let _=x;}",
            "clock",
        ),
        (
            "type Identity<T> = T; pub fn value(x:Identity<core::sync::atomic::AtomicU64>){let _=x;}",
            "shared-state",
        ),
        (
            "type Identity<T> = Unknown<T>; pub fn value()->Identity<u8>{7}",
            "resolution-unresolved",
        ),
    ] {
        let report = Fixture::source(source)?.report();
        assert!(rules(&report).contains(rule), "{source}\n{report:?}");
        assert_eq!(report.status(), "violations");
    }
    Ok(())
}

#[test]
fn compiler_tool_attribute_rejects_the_unchanged_proc_dependency() -> io::Result<()> {
    // The consumer and both manifests are the independent review's exact bytes.
    let fixture = Fixture::new(&[
        (
            "consumer/Cargo.toml",
            "[package]\nname='fresh_control'\nversion='0.0.0'\nedition='2024'\n[workspace]\n[dependencies]\nrustfmt={path=\"../tool_stub\"}\n",
        ),
        (
            "consumer/src/lib.rs",
            "#[rustfmt::skip] pub fn value()->u8{7}\n",
        ),
        (
            "tool_stub/Cargo.toml",
            "[package]\nname=\"rustfmt\"\nversion=\"0.0.0\"\nedition=\"2024\"\n[lib]\npath=\"lib.rs\"\nproc-macro=true\n[workspace]\n",
        ),
        (
            "tool_stub/lib.rs",
            "extern crate proc_macro; use proc_macro::TokenStream; #[proc_macro_attribute] pub fn skip(_:TokenStream,item:TokenStream)->TokenStream{item}\n",
        ),
    ])?;
    let report = check_resolved_paths(&[fixture.0.join("consumer")]);
    assert_eq!(report.status(), "violations", "{report:?}");
    assert!(report.unreadable.is_empty(), "{report:?}");
    assert!(
        report.findings.iter().any(|finding| {
            finding.rule == "resolution-attribute"
                && finding.severity == super::Severity::Error
                && finding.subject.contains("declared package/crate binding")
        }),
        "{report:?}"
    );
    Ok(())
}

#[test]
fn compiler_tool_context_retains_dependency_and_target_alternatives() -> io::Result<()> {
    for declaration in [
        "[dependencies]\nrustfmt={path='stub'}\n",
        "[dependencies]\n\"rustfmt\"={path='stub'}\n",
        "[dependencies.rustfmt]\npath='stub'\n",
        "[dependencies]\nrustfmt={path='stub',optional=true}\n",
        "[target.'cfg(unix)'.dependencies]\nrustfmt={path='stub'}\n",
        "[target.'cfg(not(unix))'.dependencies.rustfmt]\npath='stub'\n",
        "[dependencies]\nrustfmt={package='benign_formatter',path='stub'}\n",
    ] {
        let fixture = Fixture::new(&[
            (
                "Cargo.toml",
                &format!(
                    "[package]\nname='specimen'\nversion='0.0.0'\nedition='2024'\n[workspace]\n{declaration}"
                ),
            ),
            ("src/lib.rs", "#[rustfmt::skip] pub fn value()->u8{7}\n"),
        ])?;
        let report = fixture.report();
        assert!(
            rules(&report).contains("resolution-attribute"),
            "{declaration}\n{report:?}"
        );
        assert_eq!(report.status(), "violations", "{report:?}");
    }
    Ok(())
}

#[test]
fn compiler_tool_attributes_require_the_actual_qualifier_scope() -> io::Result<()> {
    for source in [
        "mod rustfmt {} #[rustfmt::skip] pub fn value()->u8{7}",
        "use core as rustfmt; #[rustfmt::skip] pub fn value()->u8{7}",
        "extern crate core as rustfmt; #[rustfmt::skip] pub fn value()->u8{7}",
        "type rustfmt=u8; #[rustfmt::skip] pub fn value()->u8{7}",
        "fn value()->u8 {use core as rustfmt; #[rustfmt::skip] fn inner()->u8{7} inner()}",
        "mod bridge{pub use core as rustfmt;} use bridge::*; #[rustfmt::skip] pub fn value()->u8{7}",
        "#[cfg(feature=\"optional\")] use core as rustfmt; #[rustfmt::skip] pub fn value()->u8{7}",
        "use std::fmt::*; #[rustfmt::skip] pub fn value()->u8{7}",
    ] {
        let report = Fixture::source(source)?.report();
        assert!(
            rules(&report).contains("resolution-attribute"),
            "{source}\n{report:?}"
        );
        assert_eq!(report.status(), "violations", "{report:?}");
    }
    // Type/module qualifiers keep their namespaces; value or bare-macro names
    // do not shadow a qualified compiler-tool path in this profile.
    for source in [
        "fn rustfmt(){} #[rustfmt::skip] pub fn value()->u8{7}",
        "macro_rules! rustfmt {()=>{7u8};} #[rustfmt::skip] pub fn value()->u8{7}",
        "#[cfg(test)] use core as rustfmt; #[rustfmt::skip] pub fn value()->u8{7}",
        "mod child{pub mod rustfmt{}} #[rustfmt::skip] pub fn value()->u8{7}",
    ] {
        let report = Fixture::source(source)?.report();
        assert_eq!(report.errors(), 0, "{source}\n{report:?}");
        assert!(report.unreadable.is_empty());
    }
    Ok(())
}

#[test]
fn compiler_tool_context_is_found_for_direct_nested_source() -> io::Result<()> {
    let fixture = Fixture::new(&[
        (
            "Cargo.toml",
            "[package]\nname='specimen'\nversion='0.0.0'\nedition='2024'\n[workspace]\n[dependencies]\nrustfmt={path='stub'}\n",
        ),
        ("src/lib.rs", "mod outer;"),
        ("src/outer/mod.rs", "pub mod inner;"),
        (
            "src/outer/inner.rs",
            "#[rustfmt::skip] pub fn value()->u8{7}\n",
        ),
    ])?;
    let report = check_resolved_paths(&[fixture.0.join("src/outer/inner.rs")]);
    assert!(
        rules(&report).contains("resolution-attribute"),
        "{report:?}"
    );
    assert_eq!(report.status(), "violations", "{report:?}");
    let profile = report.to_json()["resolution"]["compiler_tool_identity"].clone();
    assert_eq!(
        profile["profile"],
        "declared package namespaces or dependency-free standalone default extern prelude"
    );
    assert_eq!(
        profile["excluded_compiler_overrides"]
            .as_array()
            .map(Vec::len),
        Some(3)
    );
    assert!(report.render().contains("default extern prelude"));
    Ok(())
}

#[test]
fn compiler_tool_unknown_or_unreadable_manifest_identity_refuses() -> io::Result<()> {
    for manifest in [
        "[workspace]\n",
        "[package]\nname='specimen'\n[dependencies]\nformatter={package='rustfmt',path='stub'}\n",
        "[package]\nname='specimen'\n[dependencies]\n\"rustf\\u006dt\"={path='stub'}\n",
    ] {
        let fixture = Fixture::new(&[
            ("Cargo.toml", manifest),
            ("input.rs", "#[rustfmt::skip] pub fn value()->u8{7}\n"),
        ])?;
        let report = check_resolved_paths(&[fixture.0.join("input.rs")]);
        assert!(
            rules(&report).contains("resolution-attribute"),
            "{report:?}"
        );
    }
    let fixture = Fixture::source("#[rustfmt::skip] pub fn value()->u8{7}\n")?;
    fs::create_dir(fixture.0.join("Cargo.toml"))?;
    let report = fixture.report();
    assert!(
        report.findings.iter().any(|finding| {
            finding.rule == "resolution-attribute"
                && finding.subject.contains("unreadable manifest")
        }),
        "{report:?}"
    );
    Ok(())
}

#[test]
fn compiler_tool_context_accepts_a_binary_target_but_not_a_rustfmt_dependency() -> io::Result<()> {
    let bin = "[package]\nname='specimen'\n[[bin]]\nname='rustfmt'\npath='src/main.rs'\n";
    let fixture = Fixture::new(&[
        ("Cargo.toml", bin),
        ("input.rs", "#[rustfmt::skip] pub fn value()->u8{7}\n"),
    ])?;
    let report = check_resolved_paths(&[fixture.0.join("input.rs")]);
    assert!(
        !rules(&report).contains("resolution-attribute"),
        "{report:?}"
    );
    let shadow = "[package]\nname='specimen'\n[[bin]]\nname='tool'\npath='src/main.rs'\n[dependencies]\nrustfmt={path='stub'}\n";
    let fixture = Fixture::new(&[
        ("Cargo.toml", shadow),
        ("input.rs", "#[rustfmt::skip] pub fn value()->u8{7}\n"),
    ])?;
    let report = check_resolved_paths(&[fixture.0.join("input.rs")]);
    assert!(
        rules(&report).contains("resolution-attribute"),
        "{report:?}"
    );
    Ok(())
}

// Each child uses a real process cwd; the unfiltered test runner keeps its cwd.
fn context_child(test: &str, directory: &std::path::Path, scenario: &str) -> io::Result<()> {
    let output = std::process::Command::new(std::env::current_exe()?)
        .args([
            "--exact",
            &format!("purity::resolution_tests::{test}"),
            "--nocapture",
        ])
        .current_dir(directory)
        .env("ZENO_PURITY_RELATIVE_CONTEXT_CHILD", scenario)
        .output()?;
    assert!(
        output.status.success(),
        "{scenario}: {}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(())
}

fn context_paths(absolute: PathBuf) -> Vec<PathBuf> {
    [
        "lib.rs",
        "./lib.rs",
        "../src/lib.rs",
        ".",
        "./",
        "../src",
        "..",
    ]
    .into_iter()
    .map(PathBuf::from)
    .chain([absolute])
    .collect()
}

#[test]
fn relative_tool_context_finds_enclosing_collision() -> io::Result<()> {
    if std::env::var("ZENO_PURITY_RELATIVE_CONTEXT_CHILD").as_deref() == Ok("collision") {
        for path in context_paths(std::env::current_dir()?.join("lib.rs")) {
            let report = check_resolved_paths(std::slice::from_ref(&path));
            assert_eq!(report.status(), "violations", "{path:?}: {report:?}");
            assert!(report.unreadable.is_empty(), "{path:?}: {report:?}");
            assert!(
                report.findings.iter().any(|finding| {
                    finding.rule == "resolution-attribute"
                        && finding.subject.contains("declared package/crate binding")
                }),
                "{path:?}: {report:?}"
            );
        }
        return Ok(());
    }
    let fixture = Fixture::new(&[
        (
            "Cargo.toml",
            "[package]\nname='fresh_control'\nversion='0.0.0'\nedition='2024'\n[workspace]\n[dependencies]\nrustfmt={path=\"../tool_stub\"}\n",
        ),
        ("src/lib.rs", "#[rustfmt::skip] pub fn value()->u8{7}\n"),
    ])?;
    context_child(
        "relative_tool_context_finds_enclosing_collision",
        &fixture.0.join("src"),
        "collision",
    )
}

#[test]
fn relative_tool_context_keeps_genuine_package_and_standalone() -> io::Result<()> {
    if let Ok(scenario) = std::env::var("ZENO_PURITY_RELATIVE_CONTEXT_CHILD") {
        let absolute = std::env::current_dir()?.join("lib.rs");
        let paths = match scenario.as_str() {
            "escape" => vec![PathBuf::from("../../standalone/lib.rs")],
            "standalone" => ["lib.rs", "./lib.rs", ".", "./"]
                .into_iter()
                .map(PathBuf::from)
                .chain([absolute])
                .collect(),
            _ => context_paths(absolute),
        };
        for path in paths {
            let report = check_resolved_paths(std::slice::from_ref(&path));
            assert_eq!(report.errors(), 0, "{scenario}/{path:?}: {report:?}");
            assert!(
                report.unreadable.is_empty(),
                "{scenario}/{path:?}: {report:?}"
            );
        }
        return Ok(());
    }
    let fixture = Fixture::new(&[
        (
            "genuine/Cargo.toml",
            "[package]\nname='genuine'\nversion='0.0.0'\nedition='2024'\n[workspace]\n",
        ),
        (
            "genuine/src/lib.rs",
            "#[rustfmt::skip] pub fn value()->u8{7}\n",
        ),
        (
            "collision/Cargo.toml",
            "[package]\nname='collision'\nversion='0.0.0'\nedition='2024'\n[workspace]\n[dependencies]\nrustfmt={path='stub'}\n",
        ),
        (
            "collision/src/lib.rs",
            "#[rustfmt::skip] pub fn value()->u8{7}\n",
        ),
        (
            "standalone/lib.rs",
            "#[rustfmt::skip] pub fn value()->u8{7}\n",
        ),
    ])?;
    context_child(
        "relative_tool_context_keeps_genuine_package_and_standalone",
        &fixture.0.join("genuine/src"),
        "genuine",
    )?;
    context_child(
        "relative_tool_context_keeps_genuine_package_and_standalone",
        &fixture.0.join("standalone"),
        "standalone",
    )?;
    context_child(
        "relative_tool_context_keeps_genuine_package_and_standalone",
        &fixture.0.join("collision/src"),
        "escape",
    )
}

#[test]
fn relative_tool_context_refuses_unknown_and_unreadable_ancestors() -> io::Result<()> {
    if let Ok(scenario) = std::env::var("ZENO_PURITY_RELATIVE_CONTEXT_CHILD") {
        for path in [
            PathBuf::from("lib.rs"),
            PathBuf::from("./lib.rs"),
            PathBuf::from("."),
        ] {
            let report = check_resolved_paths(std::slice::from_ref(&path));
            assert_eq!(
                report.status(),
                "violations",
                "{scenario}/{path:?}: {report:?}"
            );
            assert!(
                report.findings.iter().any(|finding| {
                    finding.rule == "resolution-attribute"
                        && finding.subject.contains(if scenario == "unknown" {
                            "unsupported manifest identity"
                        } else {
                            "unreadable manifest"
                        })
                }),
                "{scenario}/{path:?}: {report:?}"
            );
        }
        return Ok(());
    }
    let fixture = Fixture::new(&[
        ("unknown/Cargo.toml", "[workspace]\n"),
        (
            "unknown/src/lib.rs",
            "#[rustfmt::skip] pub fn value()->u8{7}\n",
        ),
        (
            "unreadable/src/lib.rs",
            "#[rustfmt::skip] pub fn value()->u8{7}\n",
        ),
    ])?;
    fs::create_dir(fixture.0.join("unreadable/Cargo.toml"))?;
    context_child(
        "relative_tool_context_refuses_unknown_and_unreadable_ancestors",
        &fixture.0.join("unknown/src"),
        "unknown",
    )?;
    context_child(
        "relative_tool_context_refuses_unknown_and_unreadable_ancestors",
        &fixture.0.join("unreadable/src"),
        "unreadable",
    )
}

#[cfg(unix)]
#[test]
fn failed_working_directory_cannot_establish_standalone_tool_context() -> io::Result<()> {
    if std::env::var("ZENO_PURITY_RELATIVE_CONTEXT_CHILD").as_deref() == Ok("removed-cwd") {
        let directory = std::env::current_dir()?;
        let source = directory
            .parent()
            .ok_or_else(|| io::Error::other("missing parent"))?
            .join("input.rs");
        fs::remove_dir(&directory)?;
        assert!(std::env::current_dir().is_err());
        // An absolute readable source proves this refusal is the context failure.
        let report = check_resolved_paths(&[source]);
        assert_eq!(report.status(), "violations", "{report:?}");
        assert!(report.unreadable.is_empty(), "{report:?}");
        assert!(
            report.findings.iter().any(|finding| {
                finding.rule == "resolution-attribute"
                    && finding
                        .subject
                        .contains("cannot inspect current working directory")
            }),
            "{report:?}"
        );
        return Ok(());
    }
    let fixture = Fixture::source("#[rustfmt::skip] pub fn value()->u8{7}\n")?;
    fs::create_dir(fixture.0.join("removed"))?;
    context_child(
        "failed_working_directory_cannot_establish_standalone_tool_context",
        &fixture.0.join("removed"),
        "removed-cwd",
    )
}

#[cfg(unix)]
#[test]
fn parent_traversal_through_source_symlink_refuses_context() -> io::Result<()> {
    if std::env::var("ZENO_PURITY_RELATIVE_CONTEXT_CHILD").as_deref() == Ok("symlink-parent") {
        let report = check_resolved_paths(&[PathBuf::from("alias/../input.rs")]);
        assert_eq!(report.status(), "violations", "{report:?}");
        assert!(report.unreadable.is_empty(), "{report:?}");
        assert!(
            report.findings.iter().any(|finding| {
                finding.rule == "resolution-attribute"
                    && finding
                        .subject
                        .contains("parent context through symbolic link")
            }),
            "{report:?}"
        );
        return Ok(());
    }
    let fixture = Fixture::new(&[(
        "outside/input.rs",
        "#[rustfmt::skip] pub fn value()->u8{7}\n",
    )])?;
    fs::create_dir(fixture.0.join("outside/inner"))?;
    fs::create_dir(fixture.0.join("selected"))?;
    std::os::unix::fs::symlink(
        fixture.0.join("outside/inner"),
        fixture.0.join("selected/alias"),
    )?;
    context_child(
        "parent_traversal_through_source_symlink_refuses_context",
        &fixture.0.join("selected"),
        "symlink-parent",
    )
}
