//! db/mod.rs — SurrealDB embedded database.
//! Privacy moat: 100% local, zero cloud.
//! kv-surrealkv: pure Rust, persistent on disk.

use std::sync::Arc;
use surrealdb::engine::local::Db;
use surrealdb::Surreal;

pub type DbConn = Arc<Surreal<Db>>;

/// Initialize SurrealDB with SurrealKV backend (persistent).
pub async fn init(path: &str) -> Result<DbConn, surrealdb::Error> {
    use surrealdb::engine::local::SurrealKv;
    let db = Surreal::new::<SurrealKv>(path).await?;
    db.use_ns("creator_os").use_db("main").await?;
    Ok(Arc::new(db))
}

/// Initialize in-memory DB for tests.
pub async fn init_test() -> Result<DbConn, surrealdb::Error> {
    use surrealdb::engine::local::Mem;
    let db = Surreal::new::<Mem>(()).await?;
    db.use_ns("creator_os").use_db("test").await?;
    Ok(Arc::new(db))
}

pub mod schema;
pub use schema::{Project, Session, Track};
