use super::*;

const TOKEN: &str = "tag-limit-token";

struct Fixture {
    _workspace: TestWorkspace,
    pool: SqlitePool,
    app: Router,
    tags: Vec<Uuid>,
    assets: Vec<Uuid>,
}

impl Fixture {
    async fn new(asset_count: usize) -> Self {
        let workspace = TestWorkspace::new();
        let (state, pool) = test_state(&workspace.database(), &workspace.data()).await;
        let (account, device) = seed_account(&pool, "blobs/tag-limit", "tag-limit").await;
        sqlx::query("UPDATE devices SET token_hash = ? WHERE id = ?")
            .bind(sha2::Sha256::digest(TOKEN.as_bytes()).to_vec())
            .bind(device)
            .execute(&pool)
            .await
            .unwrap();
        let mut assets = Vec::new();
        for index in 0..asset_count {
            let upload = seed_received_upload(
                &state,
                account,
                device,
                b"tag-limit-fixture",
                &format!("tag-limit-{index}"),
            )
            .await;
            let committed = upload_commit::complete(&state, upload, account)
                .await
                .unwrap();
            assets.push(committed.asset_id);
        }
        let app = test_router(state).await;
        let mut tags = Vec::new();
        for index in 0..18 {
            let tag = json_body(
                send(
                    &app,
                    json_request(
                        Method::POST,
                        "/v1/tags",
                        json!({"name": format!("tag-{index}")}),
                        Some(TOKEN),
                        None,
                    ),
                    StatusCode::CREATED,
                )
                .await,
            )
            .await;
            tags.push(Uuid::parse_str(tag["tag_id"].as_str().unwrap()).unwrap());
        }
        Self {
            _workspace: workspace,
            pool,
            app,
            tags,
            assets,
        }
    }

    fn action(&self, method: Method, tag: usize, asset: usize) -> Request<Body> {
        empty_action_request(
            method,
            format!("/v1/tags/{}/assets/{}", self.tags[tag], self.assets[asset]),
            TOKEN,
        )
    }

    fn replacement(&self, tag: usize, assets: &[usize]) -> Request<Body> {
        json_request(
            Method::PUT,
            format!("/v1/tags/{}/assets", self.tags[tag]),
            json!({"asset_ids": assets.iter().map(|i| self.assets[*i]).collect::<Vec<_>>()}),
            Some(TOKEN),
            None,
        )
    }

    async fn seed_relation(&self, tag: usize, asset: usize) {
        sqlx::query("INSERT INTO tag_assets(tag_id, asset_id, added_at) VALUES (?, ?, '2000-01-01 00:00:00')")
            .bind(self.tags[tag])
            .bind(self.assets[asset])
            .execute(&self.pool)
            .await
            .unwrap();
    }

    async fn count(&self, asset: usize) -> i64 {
        sqlx::query_scalar("SELECT COUNT(*) FROM tag_assets WHERE asset_id = ?")
            .bind(self.assets[asset])
            .fetch_one(&self.pool)
            .await
            .unwrap()
    }

    async fn relations(&self) -> Vec<(Uuid, Uuid, String)> {
        sqlx::query_as(
            "SELECT tag_id, asset_id, added_at FROM tag_assets ORDER BY tag_id, asset_id",
        )
        .fetch_all(&self.pool)
        .await
        .unwrap()
    }

    async fn reject(&self, request: Request<Body>) {
        let before = self.relations().await;
        let counts = table_counts(&self.pool, &["account_changes", "audit_events"]).await;
        let response = send(&self.app, request, StatusCode::BAD_REQUEST).await;
        let error = json_body(response).await;
        assert_eq!(error["code"], "bad_request");
        assert_eq!(error["message"], "asset cannot have more than 16 tags");
        assert_eq!(error["retryable"], false);
        assert_eq!(
            self.relations().await,
            before,
            "rejection changed relations"
        );
        assert_eq!(
            table_counts(&self.pool, &["account_changes", "audit_events"]).await,
            counts,
            "rejection emitted changes or audit events"
        );
    }
}

#[tokio::test]
async fn tag_asset_limit_single_add_is_idempotent_and_legacy_rows_remain_removable() {
    let f = Fixture::new(1).await;
    for tag in 0..16 {
        send(
            &f.app,
            f.action(Method::POST, tag, 0),
            StatusCode::NO_CONTENT,
        )
        .await;
    }
    assert_eq!(f.count(0).await, 16);
    send(&f.app, f.action(Method::POST, 0, 0), StatusCode::NO_CONTENT).await;
    assert_eq!(f.count(0).await, 16);
    f.reject(f.action(Method::POST, 16, 0)).await;

    send(
        &f.app,
        f.action(Method::DELETE, 0, 0),
        StatusCode::NO_CONTENT,
    )
    .await;
    send(
        &f.app,
        f.action(Method::POST, 16, 0),
        StatusCode::NO_CONTENT,
    )
    .await;
    assert_eq!(f.count(0).await, 16);

    // Simulate pre-limit data only in this temporary fixture. Normal endpoints
    // must not increase it, but reads, repeats, and removals must remain usable.
    f.seed_relation(17, 0).await;
    let asset = get_json(&f.app, format!("/v1/assets/{}", f.assets[0]), TOKEN).await;
    assert_eq!(asset["tag_names"].as_array().unwrap().len(), 17);
    let tags = get_json(&f.app, "/v1/tags", TOKEN).await;
    assert_eq!(tags.as_array().unwrap().len(), 18);
    send(&f.app, f.action(Method::POST, 1, 0), StatusCode::NO_CONTENT).await;
    send(&f.app, f.replacement(1, &[0, 0]), StatusCode::NO_CONTENT).await;
    assert_eq!(f.count(0).await, 17);
    f.reject(f.action(Method::POST, 0, 0)).await;
    f.reject(f.replacement(0, &[0, 0])).await;
    send(&f.app, f.replacement(17, &[]), StatusCode::NO_CONTENT).await;
    send(
        &f.app,
        f.action(Method::DELETE, 16, 0),
        StatusCode::NO_CONTENT,
    )
    .await;
    send(
        &f.app,
        f.action(Method::DELETE, 16, 0),
        StatusCode::NO_CONTENT,
    )
    .await;
    send(&f.app, f.action(Method::POST, 0, 0), StatusCode::NO_CONTENT).await;
    assert_eq!(f.count(0).await, 16);
}

#[tokio::test]
async fn tag_asset_limit_bulk_replacement_rejects_atomically_and_counts_distinct_relations() {
    let f = Fixture::new(4).await;
    // Asset 0 is full, asset 1 has one slot, and assets 2 and 3 are existing
    // members of tag 16. Asset 3 is also full including that same tag.
    for tag in 0..16 {
        f.seed_relation(tag, 0).await;
    }
    for tag in 0..15 {
        f.seed_relation(tag, 1).await;
        f.seed_relation(tag, 3).await;
    }
    f.seed_relation(16, 2).await;
    f.seed_relation(16, 3).await;
    f.reject(f.replacement(16, &[3, 1, 1, 0])).await;
    assert_eq!(f.count(2).await, 1, "failed replacement removed old member");
    assert_eq!(f.count(1).await, 15, "failed replacement kept partial add");

    send(
        &f.app,
        f.replacement(16, &[3, 1, 1]),
        StatusCode::NO_CONTENT,
    )
    .await;
    assert_eq!(f.count(1).await, 16);
    assert_eq!(f.count(2).await, 0);
    assert_eq!(f.count(3).await, 16);
    send(
        &f.app,
        f.replacement(16, &[1, 3, 1, 3]),
        StatusCode::NO_CONTENT,
    )
    .await;
    assert_eq!(f.count(1).await, 16);
    assert_eq!(f.count(3).await, 16);
    let tag = get_json(&f.app, "/v1/tags", TOKEN)
        .await
        .as_array()
        .unwrap()
        .iter()
        .find(|tag| tag["tag_id"] == f.tags[16].to_string())
        .unwrap()
        .clone();
    assert_eq!(tag["asset_count"], 2);
    send(&f.app, f.replacement(16, &[]), StatusCode::NO_CONTENT).await;
    assert_eq!(f.count(1).await, 15);
    assert_eq!(f.count(3).await, 15);
}

#[tokio::test]
async fn tag_asset_limit_concurrent_single_and_bulk_add_share_the_last_slot() {
    let f = Fixture::new(1).await;
    for tag in 0..15 {
        f.seed_relation(tag, 0).await;
    }
    let (single, bulk) = tokio::join!(
        f.app.clone().oneshot(f.action(Method::POST, 15, 0)),
        f.app.clone().oneshot(f.replacement(16, &[0, 0]))
    );
    let single = single.unwrap();
    let bulk = bulk.unwrap();
    assert!(
        (single.status() == StatusCode::NO_CONTENT && bulk.status() == StatusCode::BAD_REQUEST)
            || (single.status() == StatusCode::BAD_REQUEST
                && bulk.status() == StatusCode::NO_CONTENT),
        "only one new relation should win the last slot: single={}, bulk={}",
        single.status(),
        bulk.status()
    );
    assert_eq!(f.count(0).await, 16);
}
