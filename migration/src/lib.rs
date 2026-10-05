#![allow(elided_lifetimes_in_paths)]
#![allow(clippy::wildcard_imports)]
pub use sea_orm_migration::prelude::*;
mod m20220101_000001_users;

mod m20261004_164825_os_versions;
mod m20261004_165153_machines;
mod m20261004_165311_settings;
mod m20261005_141635_add_packages_to_machines;
mod m20261005_215500_add_timezone_to_machines;
mod m20261005_221000_add_password_hash_to_machines;
mod m20261005_223500_add_install_started_at_to_machines;
pub struct Migrator;

#[async_trait::async_trait]
impl MigratorTrait for Migrator {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        vec![
            Box::new(m20220101_000001_users::Migration),
            Box::new(m20261004_164825_os_versions::Migration),
            Box::new(m20261004_165153_machines::Migration),
            Box::new(m20261004_165311_settings::Migration),
            Box::new(m20261005_141635_add_packages_to_machines::Migration),
            Box::new(m20261005_215500_add_timezone_to_machines::Migration),
            Box::new(m20261005_221000_add_password_hash_to_machines::Migration),
            Box::new(m20261005_223500_add_install_started_at_to_machines::Migration),
            // inject-above (do not remove this comment)
        ]
    }
}
