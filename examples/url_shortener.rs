use std::time::Duration;

use async_recursion::async_recursion;
use axum::{
    async_trait,
    body::Body,
    extract::{FromRef, FromRequestParts, Path, State},
    http::{request::Parts, StatusCode},
    response::{IntoResponse, Redirect, Response},
    routing::{get, post},
    Json, Router,
};
use nanoid::nanoid;
use serde::Deserialize;
use sqlx::{postgres::PgPoolOptions, PgPool};
use tracing::info;

#[tokio::main]
async fn main() {
    // initialize tracing
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .init();

    let db_connection_str = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://postgres:root@localhost".to_string());

    // set up connection pool
    let pool = PgPoolOptions::new()
        .max_connections(5)
        .acquire_timeout(Duration::from_secs(3))
        .connect(&db_connection_str)
        .await
        .expect("can't connect to database");

    // build our application with a route
    let app: Router = Router::new()
        // `GET /` goes to `root`
        .route("/url-shortener/:id", get(get_and_redirect))
        // `POST /users` goes to `create_user`
        .route("/url-shortener", post(create_url))
        .with_state(pool);

    // run our app with hyper, listening globally on port 3000
    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

async fn get_and_redirect(
    Path(id): Path<String>,
    State(pool): State<PgPool>,
) -> Result<Response<Body>, (StatusCode, String)> {
    info!("id: {}", id);
    let full_url_res: Result<String, sqlx::Error> =
        sqlx::query_scalar("select full_url from urls where unique_key = $1")
            .bind(&id)
            .fetch_one(&pool)
            .await;
    match full_url_res {
        Ok(full_url) => {
            info!("full_url: {}", full_url);
            let redirect = Redirect::to(&full_url);
            Ok(redirect.into_response())
        }
        Err(e) => {
            info!("error: {}", e);
            Err((StatusCode::NOT_FOUND, "not found".to_string()))
        }
    }
}

async fn create_url(
    State(pool): State<PgPool>,
    Json(payload): Json<CreateUrl>,
) -> Result<(StatusCode, String), (StatusCode, String)> {
    // insert your application logic here
    info!("full_url: {}", payload.full_url);
    //生成nano id
    let res = insert_url(&pool, &payload.full_url, 0).await;
    match res {
        Ok(unique_key) => Ok((
            StatusCode::CREATED,
            format!("{{\"unique_key\": \"{}\"}}", unique_key),
        )),
        Err(e) => Err((StatusCode::INTERNAL_SERVER_ERROR, e.to_string())),
    }
}

#[async_recursion]
async fn insert_url(pool: &PgPool, full_url: &str, count: u8) -> Result<String, anyhow::Error> {
    if count > 3 {
        return Err(anyhow::anyhow!("retried 3 times"));
    }
    let nano_id = nanoid!();
    let res: Result<sqlx::postgres::PgQueryResult, sqlx::Error> =
        sqlx::query("insert into urls (unique_key, full_url) values ($1, $2)")
            .bind(&nano_id)
            .bind(full_url)
            .execute(pool)
            .await;
    match res {
        Ok(_) => Ok(nano_id),
        Err(e) => match e {
            sqlx::Error::Database(db_err) => {
                if let Some(code) = db_err.code() {
                    if code == "23505" {
                        return insert_url(pool, full_url, count + 1).await;
                    }
                }
                Err(anyhow::anyhow!(db_err))
            }
            _ => Err(anyhow::anyhow!(e)),
        },
    }
}

// the input to our `create_user` handler
#[derive(Deserialize)]
struct CreateUrl {
    full_url: String,
}

#[allow(dead_code)]
struct DatabaseConnection(sqlx::pool::PoolConnection<sqlx::Postgres>);

#[async_trait]
impl<S> FromRequestParts<S> for DatabaseConnection
where
    PgPool: FromRef<S>,
    S: Send + Sync,
{
    type Rejection = (StatusCode, String);

    async fn from_request_parts(_parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let pool = PgPool::from_ref(state);

        let conn = pool.acquire().await.map_err(internal_error)?;

        Ok(Self(conn))
    }
}

fn internal_error<E>(err: E) -> (StatusCode, String)
where
    E: std::error::Error,
{
    (StatusCode::INTERNAL_SERVER_ERROR, err.to_string())
}
