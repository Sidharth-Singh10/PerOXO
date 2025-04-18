use dotenv::dotenv;
use matcher::matcher_service_server::{MatcherService, MatcherServiceServer};
use matcher::{GetMatchedUsersRequest, GetMatchedUsersResponse};
use sqlx::{FromRow, PgPool};
use tonic::{Request, Response, Status, transport::Server};
pub mod matcher {
    tonic::include_proto!("matcher");
}

#[derive(Debug)]
pub struct MyMatcherService {
    db: PgPool,
}

#[derive(Debug, FromRow)]
struct MatchRow {
    other_user_id: i32,
}

#[tonic::async_trait]
impl MatcherService for MyMatcherService {
    async fn get_matched_users(
        &self,
        request: Request<GetMatchedUsersRequest>,
    ) -> Result<Response<GetMatchedUsersResponse>, Status> {
        let user_id = request.into_inner().user_id;

        let rows = sqlx::query_as::<_, MatchRow>(
            r#"
            SELECT
                CASE
                    WHEN male_id = $1 THEN female_id
                    ELSE male_id
                END as other_user_id
            FROM matches
            WHERE (male_id = $1 OR female_id = $1) AND status = 'matched';
            "#,
        )
        .bind(user_id)
        .fetch_all(&self.db)
        .await
        .map_err(|e| Status::internal(format!("Database error: {}", e)))?;

        let matched_user_ids = rows.into_iter().map(|r| r.other_user_id).collect();

        Ok(Response::new(GetMatchedUsersResponse { matched_user_ids }))
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    dotenv().ok();

    let addr = "[::1]:50051".parse()?;
    let db_url = std::env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    let db = PgPool::connect(&db_url).await?;

    println!("MatcherService gRPC server running on {}", addr);

    Server::builder()
        .add_service(MatcherServiceServer::new(MyMatcherService { db }))
        .serve(addr)
        .await?;

    Ok(())
}
