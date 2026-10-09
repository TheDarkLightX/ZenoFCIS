//! The actual SQLite shell library's pure admission and migration APIs.
//! Authoring uses the same library entry points as a store upgrade.

#[cfg(test)]
pub(crate) use zeno_fcis_shell_sqlite::v2::behaviour;
pub(crate) use zeno_fcis_shell_sqlite::v2::migration;
