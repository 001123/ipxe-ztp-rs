use loco_rs::schema::*;
use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, m: &SchemaManager) -> Result<(), DbErr> {
        create_table(
            m,
            "machines",
            &[
                ("id", ColType::PkAuto),
                ("mac", ColType::StringUniq),
                ("name", ColType::StringNull),
                ("status", ColType::String),
                ("last_seen_at", ColType::DateTimeNull),
                ("approved_at", ColType::DateTimeNull),
                ("installed_at", ColType::DateTimeNull),
                ("notes", ColType::TextNull),
                ("username", ColType::StringNull),
                ("ssh_key", ColType::TextNull),
                ("install_disk", ColType::StringNull),
                ("cloudinit_url", ColType::StringNull),
            ],
            &[("os_version?", "")],
        )
        .await
    }

    async fn down(&self, m: &SchemaManager) -> Result<(), DbErr> {
        drop_table(m, "machines").await
    }
}
