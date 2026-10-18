fn main() {
    let result = (|| {
        if std::env::args_os().len() > 1 {
            return Err(
                "U128SubjectUnsupported: SQLite migration and scalar session commands".to_owned(),
            );
        }
        let contract = application::v2_contract::Contract::new();
        let descriptor = contract.descriptor();
        let authority = application::authority(&descriptor)?;
        let examples = application::examples(include_str!("../tests/decision-examples.txt"))?;
        for example in &examples {
            application::check(&authority, example)?
        }
        println!(
            "checked {} typed decision examples and original publication replay",
            examples.len()
        );
        Ok::<_, String>(())
    })();
    if let Err(e) = result {
        eprintln!("{e}");
        std::process::exit(1)
    }
}
