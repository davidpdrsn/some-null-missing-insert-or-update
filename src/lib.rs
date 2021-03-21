#![allow(dead_code)]

use serde::Deserialize;

type DbPool =
    bb8_postgres::bb8::Pool<bb8_postgres::PostgresConnectionManager<tokio_postgres::NoTls>>;

#[derive(Deserialize)]
struct Update {
    // double option to differentiate `null` and "missing"
    #[serde(default, deserialize_with = "deserialize_some")]
    one: Option<Option<String>>,
    #[serde(default, deserialize_with = "deserialize_some")]
    two: Option<Option<String>>,
}

// based on https://github.com/serde-rs/serde/issues/984#issuecomment-314143738
fn deserialize_some<'de, T, D>(deserializer: D) -> Result<Option<T>, D::Error>
where
    T: serde::de::Deserialize<'de>,
    D: serde::de::Deserializer<'de>,
{
    serde::de::Deserialize::deserialize(deserializer).map(Some)
}

impl Update {
    async fn insert_or_update(self, internal_id: i64, pool: &DbPool) {
        let mut con = pool.get().await.unwrap();
        let tx = con.transaction().await.unwrap();

        // check if row exists, if it does lock it so others cannot query it
        let row = tx
            .query_opt(
                r#"
                select *
                from users
                where internal_id = $1
                for update
                "#,
                &[&internal_id],
            )
            .await
            .unwrap();

        if let Some(row) = row {
            // update the existing row
            tx.execute(
                r#"
                update users
                set
                    one = $2
                    , two = $3
                where internal_id = $1
                "#,
                &[
                    &internal_id,
                    // if value wasn't specified set it to the current value
                    &self.one.unwrap_or_else(|| row.get("one")),
                    &self.two.unwrap_or_else(|| row.get("two")),
                ],
            )
            .await
            .unwrap();
        } else {
            tx.execute(
                r#"
                insert into users (internal_id, one, two)
                values ($1, $2, $3)
                "#,
                &[
                    &internal_id,
                    // null and unspecified is the same for initial insert
                    &self.one.flatten(),
                    &self.two.flatten(),
                ]
            )
            .await
            .unwrap();
        };

        tx.commit().await.unwrap();
    }
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
    use super::*;
    use bb8_postgres::{bb8, PostgresConnectionManager};
    use serde_json::json;
    use std::process::Command;

    #[tokio::test]
    async fn works() {
        let pool = db_connect().await;

        let internal_id = 1;

        // initial insert
        let payload = json!({
            "one": "1",
            "two": "1",
        });
        let payload = serde_json::from_value::<Update>(payload).unwrap();
        payload.insert_or_update(internal_id, &pool).await;

        let user = fetch(&pool, internal_id).await;
        assert_eq!(user.internal_id, 1);
        assert_eq!(user.one.as_deref(), Some("1"));
        assert_eq!(user.two.as_deref(), Some("1"));

        // updating both
        let payload = json!({
            "one": "2",
            "two": "2",
        });
        let payload = serde_json::from_value::<Update>(payload).unwrap();
        payload.insert_or_update(internal_id, &pool).await;

        let user = fetch(&pool, internal_id).await;
        assert_eq!(user.internal_id, 1);
        assert_eq!(user.one.as_deref(), Some("2"));
        assert_eq!(user.two.as_deref(), Some("2"));

        // updating one
        let payload = json!({
            "one": "3",
        });
        let payload = serde_json::from_value::<Update>(payload).unwrap();
        payload.insert_or_update(internal_id, &pool).await;

        let user = fetch(&pool, internal_id).await;
        assert_eq!(user.one.as_deref(), Some("3"));
        assert_eq!(user.two.as_deref(), Some("2"));

        // updating the other
        let payload = json!({
            "two": "3",
        });
        let payload = serde_json::from_value::<Update>(payload).unwrap();
        payload.insert_or_update(internal_id, &pool).await;

        let user = fetch(&pool, internal_id).await;
        assert_eq!(user.one.as_deref(), Some("3"));
        assert_eq!(user.two.as_deref(), Some("3"));

        // updating neither
        let payload = json!({});
        let payload = serde_json::from_value::<Update>(payload).unwrap();
        payload.insert_or_update(internal_id, &pool).await;

        let user = fetch(&pool, internal_id).await;
        assert_eq!(user.one.as_deref(), Some("3"));
        assert_eq!(user.two.as_deref(), Some("3"));

        // setting one to `null`
        let payload = json!({ "one": null });
        let payload = serde_json::from_value::<Update>(payload).unwrap();
        payload.insert_or_update(internal_id, &pool).await;

        let user = fetch(&pool, internal_id).await;
        assert_eq!(user.one.as_deref(), None, "one == null");
        assert_eq!(user.two.as_deref(), Some("3"));

        // change one, set two to null
        let payload = json!({ "one": "1", "two": null });
        let payload = serde_json::from_value::<Update>(payload).unwrap();
        payload.insert_or_update(internal_id, &pool).await;

        let user = fetch(&pool, internal_id).await;
        assert_eq!(user.one.as_deref(), Some("1"));
        assert_eq!(user.two.as_deref(), None);
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
