use app::{
    app::App,
    models::users::{self, RegisterParams},
    sql,
};
use loco_rs::testing::prelude::*;
use serial_test::serial;

/// SQLx runs on the pool Sea-ORM opened (`src/sql.rs`): a row Sea-ORM writes
/// is one SQLx reads. `query_scalar!` on purpose, so the compile-time check
/// and the `.sqlx/` cache are exercised from the first commit on.
#[tokio::test]
#[serial]
async fn sqlx_reads_what_sea_orm_wrote() {
    let boot = boot_test::<App>()
        .await
        .expect("Failed to boot test application");
    let ctx = &boot.app_context;

    users::Model::create_with_password(
        &ctx.db,
        &RegisterParams {
            email: "pool@example.com".into(),
            password: "12341234".into(),
            name: "Pool".into(),
        },
    )
    .await
    .expect("Sea-ORM writes the user");

    let email = "pool@example.com";
    let count = sqlx::query_scalar!(
        r#"SELECT COUNT(*) AS "count!: i64" FROM users WHERE email = ?"#,
        email
    )
    .fetch_one(sql::pool(ctx))
    .await
    .expect("SQLx reads through the same pool");
    assert_eq!(count, 1);
}
