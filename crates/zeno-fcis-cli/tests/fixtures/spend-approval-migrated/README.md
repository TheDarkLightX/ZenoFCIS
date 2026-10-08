# Spend-approval application after a data migration

The application `zeno-fcis new --contract ../spend-approval` creates, after
`zeno-fcis contract evolve --to ../spend-approval-priority --migration
../spend-approval-priority/migration.json`. Every file is generated or
retained by those two commands; none was edited.

- Version 1 is the spend-approval contract, kept under `v2/evolutions/1/`
  with the plain-language diff of the change, `review.txt`, and the
  migration, `migration.json`. Its source is `src/v2_contract_v1.rs`, which
  reads its own schema, `v2/schema_v1.zcve`, because the migration changed
  the schema.
- Version 2 is the spend-approval-priority contract: the state gains the
  urgent flag, which every case carries over. `src/v2_contract.rs` declares
  the migration in `STATE_STEPS`.

The SQLite shell's `tests/data_migration.rs` compiles `src/v2_contract.rs`
to upgrade version 1 stores across the migration, and the CLI's
`tests/contract_migrate.rs` checks that `generate contract --check` still
finds every file current.
