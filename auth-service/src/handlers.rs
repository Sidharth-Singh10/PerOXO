use axum::{Extension, Json, http::StatusCode};
use serde::Serialize;
use sqlx::PgPool;

use crate::{db::insert_tenant_keypair, tenant};

#[derive(Serialize)]
pub struct GenerateTenantResponse {
    project_id: String,
    secret_api_key: String,
}

pub async fn generate_tenant_handler(
    Extension(pool): Extension<PgPool>,
) -> Result<Json<GenerateTenantResponse>, (StatusCode, String)> {
    let kp = tenant::TenantKeypair::new();

    let project_id = kp.get_project_id().clone();
    let secret_api_key = kp.get_secret_api_key().clone();

    insert_tenant_keypair(&pool, &project_id, &secret_api_key)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let resp = GenerateTenantResponse {
        project_id,
        secret_api_key,
    };

    Ok(Json(resp))
}
