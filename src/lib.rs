#![allow(warnings)]

use bb8_postgres::{bb8, PostgresConnectionManager};
use serde::Deserialize;
use serde_json::json;
use std::process::Command;

mod patch;

type DbPool =
    bb8_postgres::bb8::Pool<bb8_postgres::PostgresConnectionManager<tokio_postgres::NoTls>>;

#[derive(Deserialize)]
struct Update {
    one: Option<String>,
    two: Option<String>,
}

async fn insert_or_update(pool: &DbPool, internal_id: i64, update: Update) {
    let con = pool.get().await.unwrap();

    con.execute(
        r#"
        insert into users (internal_id, one, two)
        values ($1, $2, $3)

        on conflict (internal_id)
        do update set
            one = coalesce(excluded.one, users.one)
            , two = coalesce(excluded.two, users.two)
        "#,
        &[&internal_id, &update.one, &update.two],
    )
    .await
    .unwrap();
}

struct User {
    id: i64,
    internal_id: i64,
    one: Option<String>,
    two: Option<String>,
}

async fn fetch(pool: &DbPool, internal_id: i64) -> User {
    let con = pool.get().await.unwrap();

    let row = con
        .query_one(
            "select * from users where internal_id = $1",
            &[&internal_id],
        )
        .await
        .unwrap();

    User {
        id: row.get("id"),
        internal_id: row.get("internal_id"),
        one: row.get("one"),
        two: row.get("two"),
    }
}

#[cfg(test)]
mod tests {
    #[allow(unused_imports)]
    use super::*;

    #[tokio::test]
    #[ignore]
    async fn works() {
        let pool = db_connect().await;

        let internal_id = 1;

        // initial insert
        let payload = json!({
            "one": "1",
            "two": "1",
        });
        let payload = serde_json::from_value(payload).unwrap();
        insert_or_update(&pool, internal_id, payload).await;

        let user = fetch(&pool, internal_id).await;
        assert_eq!(user.internal_id, 1);
        assert_eq!(user.one.as_deref(), Some("1"));
        assert_eq!(user.two.as_deref(), Some("1"));

        // updating both
        let payload = json!({
            "one": "2",
            "two": "2",
        });
        let payload = serde_json::from_value(payload).unwrap();
        insert_or_update(&pool, internal_id, payload).await;

        let user = fetch(&pool, internal_id).await;
        assert_eq!(user.internal_id, 1);
        assert_eq!(user.one.as_deref(), Some("2"));
        assert_eq!(user.two.as_deref(), Some("2"));

        // updating one
        let payload = json!({
            "one": "3",
        });
        let payload = serde_json::from_value(payload).unwrap();
        insert_or_update(&pool, internal_id, payload).await;

        let user = fetch(&pool, internal_id).await;
        assert_eq!(user.one.as_deref(), Some("3"));
        assert_eq!(user.two.as_deref(), Some("2"));

        // updating the other
        let payload = json!({
            "two": "3",
        });
        let payload = serde_json::from_value(payload).unwrap();
        insert_or_update(&pool, internal_id, payload).await;

        let user = fetch(&pool, internal_id).await;
        assert_eq!(user.one.as_deref(), Some("3"));
        assert_eq!(user.two.as_deref(), Some("3"));

        // updating neither
        let payload = json!({});
        let payload = serde_json::from_value(payload).unwrap();
        insert_or_update(&pool, internal_id, payload).await;

        let user = fetch(&pool, internal_id).await;
        assert_eq!(user.one.as_deref(), Some("3"));
        assert_eq!(user.two.as_deref(), Some("3"));

        // setting one to `null`
        let payload = json!({ "one": null });
        let payload = serde_json::from_value(payload).unwrap();
        insert_or_update(&pool, internal_id, payload).await;

        let user = fetch(&pool, internal_id).await;
        assert_eq!(user.one.as_deref(), None, "one == null");
        assert_eq!(user.two.as_deref(), Some("3"));

    }

    async fn db_connect() -> DbPool {
        assert!(Command::new("./setup").status().unwrap().success());

        let mut config = tokio_postgres::config::Config::new();

        config.host("localhost");
        config.user("david.pedersen");
        config.dbname("testing");

        let manager = PostgresConnectionManager::new(config, tokio_postgres::NoTls);

        bb8::Pool::builder()
            .max_size(32)
            .build(manager)
            .await
            .unwrap()
    }
}
