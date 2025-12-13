use sqlx::Row;
use sqlx::{PgPool, Result};
use tracing::{info, instrument};

#[instrument(skip(pool, secret_key), fields(project_id = %project_id))]
pub async fn insert_tenant_keypair(
    pool: &PgPool,
    project_id: &str,
    secret_key: &str,
) -> Result<()> {
    sqlx::query(
        r#"
        INSERT INTO tenant_keys (project_id, secret_api_key)
        VALUES ($1, $2)
        "#,
    )
    .bind(project_id)
    .bind(secret_key)
    .execute(pool)
    .await?;

    info!("inserted tenant keypair");

    Ok(())
}

#[instrument(skip(pool), fields(project_id = %project_id))]
pub async fn get_secret_key_by_project_id(
    pool: &PgPool,
    project_id: &str,
) -> Result<Option<String>> {
    let q = "SELECT project_id, secret_api_key FROM tenant_keys WHERE project_id = $1";

    let query = sqlx::query(q).bind(project_id);
    let row = query.fetch_optional(pool).await?;

    if let Some(row) = row {
        let secret_api_key: String = row.get("secret_api_key");
        info!("found secret key for project_id");
        Ok(Some(secret_api_key))
    } else {
        info!("no secret key found for project_id");
        Ok(None)
    }
}
