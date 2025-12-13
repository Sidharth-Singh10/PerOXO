use std::net::SocketAddr;
use tonic::{Request, Response, Status};
use tracing::{error, info, instrument};

use crate::user_token as user_token_module;

tonic::include_proto!("auth_service");

#[derive(Clone)]
pub struct AuthServiceImpl {
    pub redis_client: redis::Client,
}

#[tonic::async_trait]
impl auth_service_server::AuthService for AuthServiceImpl {
    #[instrument(skip(self, request))]
    async fn verify_user_token(
        &self,
        request: Request<VerifyUserTokenRequest>,
    ) -> Result<Response<VerifyUserTokenResponse>, Status> {
        let token = request.into_inner().token;

        match user_token_module::verify_user_token(&self.redis_client, &token).await {
            Ok(Some(t)) => {
                // clone values for logging without moving `t` before we build response
                let project_id_val = t.project_id.clone();
                let user_id_val = t.user_id.clone();
                info!(token = ?token, project_id = ?project_id_val, user_id = ?user_id_val, "token verified");

                let user_token = UserToken {
                    project_id: t.project_id,
                    user_id: t.user_id,
                    expires_at: t.expires_at,
                };

                let resp = VerifyUserTokenResponse {
                    found: true,
                    user_token: Some(user_token),
                };

                Ok(Response::new(resp))
            }
            Ok(None) => {
                info!(token = ?token, "token not found or invalid");
                Ok(Response::new(VerifyUserTokenResponse {
                    found: false,
                    user_token: None,
                }))
            }
            Err(e) => {
                error!(%e, "error verifying token");
                Err(Status::internal(e.to_string()))
            }
        }
    }
}

pub async fn start_grpc_server(
    addr: SocketAddr,
    redis_client: redis::Client,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let svc = auth_service_server::AuthServiceServer::new(AuthServiceImpl { redis_client });

    info!(addr = ?addr, "starting gRPC server");

    tonic::transport::Server::builder()
        .add_service(svc)
        .serve(addr)
        .await?;

    Ok(())
}
