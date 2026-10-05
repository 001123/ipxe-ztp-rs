use loco_rs::schema::*;
use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, m: &SchemaManager) -> Result<(), DbErr> {
        create_table(
            m,
            "os_versions",
            &[
                ("id", ColType::PkAuto),
                ("name", ColType::String),
                ("version", ColType::String),
                ("arch", ColType::String),
                ("kernel_url", ColType::Text),
                ("initrd_url", ColType::Text),
                ("iso_url", ColType::Text),
                ("download_status", ColType::String),
                ("bytes_downloaded", ColType::BigInteger),
                ("total_bytes", ColType::BigInteger),
                ("error", ColType::TextNull),
                ("boot_mode", ColType::String),
            ],
            &[],
        )
        .await
    }

    async fn down(&self, m: &SchemaManager) -> Result<(), DbErr> {
        drop_table(m, "os_versions").await
    }
}
