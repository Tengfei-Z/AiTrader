pub mod account;
pub mod market;
pub mod model;

use axum::Router;

use crate::AppState;

pub fn api_routes() -> Router<AppState> {
    Router::new()
        .nest("/market", market::router())
        .nest("/account", account::router())
        .nest("/model", model::router())
}

pub use account::run_balance_snapshot_loop;
