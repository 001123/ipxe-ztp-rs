use ipxe_loco_rs::{app::App, models::os_versions};
use loco_rs::testing::prelude::*;
use sea_orm::EntityTrait;
use serial_test::serial;

macro_rules! configure_insta {
    ($($expr:expr),*) => {
        let mut settings = insta::Settings::clone_current();
        settings.set_prepend_module_to_snapshot(false);
        let _guard = settings.bind_to_scope();
    };
}

/// The `OsVersions` entity matches the table its migration created.
///
/// Selecting every column is the cheapest assertion that says something true:
/// it fails if the migration and the entity disagree — a renamed column, a
/// type that does not round-trip, a migration that never ran — which is the
/// most common way a generated model breaks. Extend it as the model grows.
#[tokio::test]
#[serial]
async fn can_query_os_versions() {
    configure_insta!();

    let boot = boot_test::<App>().await.unwrap();
    seed::<App>(&boot.app_context).await.unwrap();

    // The query is the assertion. Bind the result and compare it once you have
    // seed data to compare against, e.g.:
    //
    // let items = os_versions::Entity::find().all(&boot.app_context.db).await.unwrap();
    // assert_debug_snapshot!(items);
    os_versions::Entity::find()
        .all(&boot.app_context.db)
        .await
        .expect("`os_versions` should be queryable — entity and migration must agree");
}

#[test]
fn file_name_from_url_takes_the_last_segment() {
    assert_eq!(
        os_versions::Model::file_name_from_url(
            "http://releases.ubuntu.com/24.04/ubuntu-24.04-live-server-amd64.iso"
        ),
        Some("ubuntu-24.04-live-server-amd64.iso".to_string())
    );
    assert_eq!(
        os_versions::Model::file_name_from_url("http://mirror.example/rocky.iso?sha256=abc"),
        Some("rocky.iso".to_string())
    );
    assert_eq!(
        os_versions::Model::file_name_from_url("http://mirror.example/"),
        None
    );
}

#[tokio::test]
#[serial]
async fn artifact_path_rejects_traversal() {
    let boot = boot_test::<App>().await.unwrap();
    seed::<App>(&boot.app_context).await.unwrap();
    let os = os_versions::ActiveModel::create(
        &boot.app_context.db,
        &os_versions::CreateParams {
            name: "Ubuntu".to_string(),
            version: "24.04".to_string(),
            arch: "amd64".to_string(),
            kernel_url: "http://m/k".to_string(),
            initrd_url: "http://m/i".to_string(),
            iso_url: "http://m/i.iso".to_string(),
            boot_mode: "online".to_string(),
        },
    )
    .await
    .unwrap();

    let data = std::path::Path::new("/data");
    assert_eq!(
        os.artifact_path(data, "vmlinuz"),
        Some(std::path::PathBuf::from(format!(
            "/data/os/{}/vmlinuz",
            os.id
        )))
    );
    assert_eq!(os.artifact_path(data, "../etc/passwd"), None);
    assert_eq!(os.artifact_path(data, "sub/dir"), None);
    assert_eq!(os.artifact_path(data, ""), None);
}

#[tokio::test]
#[serial]
async fn iso_file_name_is_derived_from_iso_url() {
    let boot = boot_test::<App>().await.unwrap();
    seed::<App>(&boot.app_context).await.unwrap();
    let os = os_versions::ActiveModel::create(
        &boot.app_context.db,
        &os_versions::CreateParams {
            name: "Ubuntu".to_string(),
            version: "26.04".to_string(),
            arch: "amd64".to_string(),
            kernel_url: "http://m/k".to_string(),
            initrd_url: "http://m/i".to_string(),
            iso_url: "http://m/ubuntu-26.04-live-server-amd64.iso".to_string(),
            boot_mode: "online".to_string(),
        },
    )
    .await
    .unwrap();

    // the ISO is the only network artifact; kernel and initrd are extracted
    // from it, so their mirror URLs are no longer fetched
    assert_eq!(os.iso_file_name(), "ubuntu-26.04-live-server-amd64.iso");
}
