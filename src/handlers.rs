use super::{Container, DbPoolGetter};
use crate::middleware::{roundtrip, Role};
use crate::models::{self, NewUser, User, UserError};
use darpi::job::IOBlockingJob;
use darpi::{chrono::Duration, handler, Json, Path, Query};
use darpi_middleware::{auth::*, body_size_limit};
use log::warn;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Deserialize, Serialize, Debug)]
pub struct Login {
    email: String,
    password: String,
}

// here we give the container type
// so the framework knows where to get
// the requested `Arc<dyn JwtTokenCreator>` from
#[handler({
    container: Container
})]
pub(crate) async fn login(
    #[body] _login_data: Json<Login>,
    #[inject] jwt_tok_creator: Arc<dyn JwtTokenCreator>,
) -> Result<Token, Error> {
    let admin = Role::Admin; // hardcoded just for the example
    let uid = "uid"; // hardcoded just for the example
    let tok = jwt_tok_creator
        .create(uid, &admin, Duration::days(30))
        .await
        .map_err(|e| {
            warn!("could not create a token: {}", e);
            e
        })?;

    Ok(tok)
}

#[derive(Deserialize, Serialize, Debug, Query, Path)]
pub struct Name {
    name: String,
}

#[handler]
pub(crate) async fn home() -> String {
    "Welcome to darpi".to_string()
}

// here we give the container type
// so the framework knows where to get
// the requested `Arc<dyn DbPoolGetter>` from
// enforce max request body size 128 bytes and admin role via middleware
#[handler({
    container: Container,
    middleware: {
        request: [roundtrip("my string"), body_size_limit(128), authorize(Role::Admin)]
    }
})]
pub(crate) async fn create_user(
    #[body] new_user: Json<NewUser>,
    #[inject] db_pool: Arc<dyn DbPoolGetter>,
    #[middleware::request(0)] _: String,
) -> Result<Json<User>, UserError> {
    let conn = db_pool.pool().get()?;

    //diesel does not have an async api
    //we don't want to block the server thread
    //so we will offload this as a blocking task
    // to be executed on an appropriate thread
    // and we will wait for the result on an async channel
    let job = move || models::create_user(new_user.into_inner(), &conn);
    let user = darpi::oneshot(IOBlockingJob::from(job))
        .await
        .map_err(|_| UserError::InternalError)?
        .await
        .map_err(|_| UserError::InternalError)??;

    Ok(Json(user))
}

// the `from_path` attribute allows us
// to deserialize `UserID` from the request path
#[derive(Deserialize, Path)]
pub struct UserID {
    id: i32,
}

// here we give the container type
// so the framework knows where to get
// the requested `Arc<dyn DbPoolGetter>` from
#[handler({
    container: Container
})]
pub(crate) async fn get_user(
    #[path] user_id: UserID,
    #[inject] db_pool: Arc<dyn DbPoolGetter>,
) -> Result<Option<Json<User>>, UserError> {
    let conn = db_pool.pool().get()?;

    //diesel does not have an async api
    //we don't want to block the server thread
    //so we will offload this as a blocking task
    // to be executed on an appropriate thread
    // and we will wait for the result on an async channel
    let job = move || models::find_user_by_id(user_id.id, &conn);
    let user = darpi::oneshot(IOBlockingJob::from(job))
        .await
        .map_err(|_| UserError::InternalError)?
        .await
        .map_err(|_| UserError::InternalError)??;

    user.map_or(Ok(None), |u| Ok(Some(Json(u))))
}
