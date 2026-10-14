use super::*;

#[tokio::test]
async fn published_upload_retries_after_real_metadata_transaction_failure() {
    let workspace = TestWorkspace::new();
    let (state, pool) = test_state(&workspace.database(), &workspace.data()).await;
    let (account, device) = seed_account(&pool, "blobs/sql-failure", "sql-failure").await;
    // Fail both early and late in the actual SQLite metadata transaction. The
    // late failure also proves that resource and change-feed writes roll back.
    for (index, trigger) in [
        "CREATE TRIGGER reject_commit BEFORE INSERT ON resources BEGIN SELECT RAISE(ABORT, 'test metadata unavailable'); END",
        "CREATE TRIGGER reject_commit BEFORE UPDATE OF commit_state ON uploads WHEN NEW.commit_state = 'committed' BEGIN SELECT RAISE(ABORT, 'test metadata unavailable'); END",
    ].into_iter().enumerate() {
        let content = format!("published-but-not-indexed-{index}");
        let upload = seed_received_upload(&state, account, device, content.as_bytes(), &format!("sql-{index}")).await;
        sqlx::query(sqlx::AssertSqlSafe(trigger)).execute(&pool).await.unwrap();
        let error = upload_commit::complete(&state, upload, account).await.unwrap_err();
        assert_eq!(error.status, StatusCode::INTERNAL_SERVER_ERROR);
        let (status, final_key): (String, String) = sqlx::query_as("SELECT commit_state, commit_final_key FROM uploads WHERE id = ?")
            .bind(upload).fetch_one(&pool).await.unwrap();
        assert_eq!(status, "finalizing");
        assert_eq!(std::fs::read(workspace.data().join(&final_key)).unwrap(), content.as_bytes());
        let counts: (i64, i64) = sqlx::query_as("SELECT (SELECT COUNT(*) FROM resources r JOIN uploads u ON r.asset_id=u.asset_id WHERE u.id=?1), (SELECT COUNT(*) FROM blobs b JOIN uploads u ON b.id=u.commit_blob_id WHERE u.id=?1)")
            .bind(upload).fetch_one(&pool).await.unwrap();
        assert_eq!(counts, (0, 0));
        sqlx::query("DROP TRIGGER reject_commit").execute(&pool).await.unwrap();
        let committed = upload_commit::complete(&state, upload, account).await.unwrap();
        assert_eq!(upload_commit::complete(&state, upload, account).await.unwrap(), committed);
        let commits: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM audit_events WHERE action='upload.commit' AND entity_id=?")
            .bind(committed.resource_id).fetch_one(&pool).await.unwrap();
        assert_eq!(commits, 1);
        let resources: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM resources WHERE asset_id=?")
            .bind(committed.asset_id).fetch_one(&pool).await.unwrap();
        assert_eq!(resources, 1);
    }
    close_test_state(state, pool).await;
}

#[tokio::test]
async fn cancelled_complete_finishes_and_repeated_complete_preserves_receipt() {
    let workspace = TestWorkspace::new();
    let (state, pool) = test_state(&workspace.database(), &workspace.data()).await;
    let (account, device) = seed_account(&pool, "blobs/cancel-complete", "cancel-complete").await;
    let upload = seed_received_upload(
        &state,
        account,
        device,
        b"request disconnected",
        "cancel-complete",
    )
    .await;
    let guard = state.storage.lock_upload_commit(upload).await.unwrap();
    let caller_state = state.clone();
    let caller =
        tokio::spawn(async move { upload_commit::complete(&caller_state, upload, account).await });
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        loop {
            match state.completion_admission.try_acquire(device) {
                Ok(permit) => drop(permit),
                Err(_) => break,
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("completion acquired its owned work permit");
    caller.abort();
    assert!(caller.await.unwrap_err().is_cancelled());
    drop(guard);
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        loop {
            let status: String = sqlx::query_scalar("SELECT commit_state FROM uploads WHERE id=?")
                .bind(upload)
                .fetch_one(&pool)
                .await
                .unwrap();
            if status == "committed" {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("detached completion survives request cancellation");
    // Wait for cleanup to release admission before imitating a later retry.
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        loop {
            if let Ok(permit) = state.completion_admission.try_acquire(device) {
                drop(permit);
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    let first = upload_commit::complete(&state, upload, account)
        .await
        .unwrap();
    assert_eq!(
        upload_commit::complete(&state, upload, account)
            .await
            .unwrap(),
        first
    );
    close_test_state(state, pool).await;
}

fn operation_request(operation: Uuid, manifest: Value, token: &str) -> Request<Body> {
    let mut request = json_request(Method::POST, "/v1/uploads", manifest, Some(token), None);
    request
        .headers_mut()
        .insert("x-upload-operation", operation.to_string().parse().unwrap());
    request
}

fn operation_manifest(bytes: &[u8]) -> Value {
    let hash = blake3::hash(bytes).to_hex().to_string();
    let midpoint = bytes.len() / 2;
    json!({"source_asset_id":"retry-photo","source_resource_id":"original",
        "media_kind":"photo","role":"primary","filename":"photo.jpg",
        "mime_type":"image/jpeg","source_created_at_ms":1,"storage_encoding":"plain-v1",
        "content_size":bytes.len(),"content_blake3":hash,"metadata":null,
        "parts":[{"index":0,"size":midpoint,"blake3":blake3::hash(&bytes[..midpoint]).to_hex().to_string()},
                 {"index":1,"size":bytes.len()-midpoint,"blake3":blake3::hash(&bytes[midpoint..]).to_hex().to_string()}]})
}

async fn operation_token(pool: &SqlitePool, device: Uuid, token: &str) {
    sqlx::query("UPDATE devices SET token_hash=? WHERE id=?")
        .bind(sha2::Sha256::digest(token.as_bytes()).to_vec())
        .bind(device)
        .execute(pool)
        .await
        .unwrap();
}

async fn operation_part(app: &Router, operation: Uuid, index: u32, bytes: &[u8], token: &str) {
    let request = Request::builder()
        .method(Method::PUT)
        .uri(format!("/v1/uploads/{operation}/parts/{index}"))
        .header("Authorization", format!("Bearer {token}"))
        .body(Body::from(bytes.to_vec()))
        .unwrap();
    send(app, request, StatusCode::NO_CONTENT).await;
}

async fn operation_complete(app: &Router, operation: Uuid, token: &str) -> Value {
    json_body(
        send(
            app,
            json_request(
                Method::POST,
                format!("/v1/uploads/{operation}/complete"),
                json!({}),
                Some(token),
                None,
            ),
            StatusCode::OK,
        )
        .await,
    )
    .await
}

async fn operation_upload(app: &Router, operation: Uuid, bytes: &[u8], token: &str) -> Value {
    let created = json_body(
        send(
            app,
            operation_request(operation, operation_manifest(bytes), token),
            StatusCode::OK,
        )
        .await,
    )
    .await;
    if created["disposition"] == "complete" {
        return created;
    }
    assert_eq!(created["upload_id"], operation.to_string());
    let midpoint = bytes.len() / 2;
    operation_part(app, operation, 0, &bytes[..midpoint], token).await;
    operation_part(app, operation, 1, &bytes[midpoint..], token).await;
    operation_complete(app, operation, token).await
}

#[tokio::test]
async fn create_retry_cannot_restore_old_bytes_but_new_operations_can_revert_and_update_metadata() {
    let workspace = TestWorkspace::new();
    let (state, pool) = test_state(&workspace.database(), &workspace.data()).await;
    let (_, device) = seed_account(&pool, "blobs/operation-order", "operation-order").await;
    let token = "operation-order-token";
    operation_token(&pool, device, token).await;
    let app = test_router(state.clone()).await;
    let old_operation = Uuid::new_v4();
    let old = operation_manifest(b"old bytes");
    operation_upload(&app, old_operation, b"old bytes", token).await;
    let newer = operation_upload(&app, Uuid::new_v4(), b"new bytes", token).await;
    let rejected = json_body(
        send(
            &app,
            operation_request(old_operation, old.clone(), token),
            StatusCode::CONFLICT,
        )
        .await,
    )
    .await;
    assert_eq!(rejected["code"], "upload_superseded");
    let visible = get_json(
        &app,
        &format!("/v1/resources/{}", newer["resource_id"].as_str().unwrap()),
        token,
    )
    .await;
    assert_eq!(
        visible["content_blake3"],
        operation_manifest(b"new bytes")["content_blake3"]
    );
    // A real user edit back to A has a new preparation identity and is allowed.
    let reverted = operation_upload(&app, Uuid::new_v4(), b"old bytes", token).await;
    assert_eq!(reverted["disposition"], "complete");
    let visible = get_json(
        &app,
        &format!(
            "/v1/resources/{}",
            reverted["resource_id"].as_str().unwrap()
        ),
        token,
    )
    .await;
    assert_eq!(visible["content_blake3"], old["content_blake3"]);
    let metadata_operation = Uuid::new_v4();
    let mut metadata = old.clone();
    metadata["filename"] = json!("renamed.jpg");
    metadata["metadata"] = json!({"caption":"updated"});
    let updated = json_body(
        send(
            &app,
            operation_request(metadata_operation, metadata.clone(), token),
            StatusCode::OK,
        )
        .await,
    )
    .await;
    let visible = get_json(
        &app,
        &format!("/v1/resources/{}", updated["resource_id"].as_str().unwrap()),
        token,
    )
    .await;
    assert_eq!(visible["filename"], "renamed.jpg");
    assert_eq!(visible["metadata"], metadata["metadata"]);
    send(
        &app,
        operation_request(metadata_operation, old, token),
        StatusCode::CONFLICT,
    )
    .await;
    drop(app);
    close_test_state(state, pool).await;
}

#[tokio::test]
async fn create_operation_resumes_parts_after_restart_and_replays_committed_and_dedup_receipts() {
    let workspace = TestWorkspace::new();
    let (state, pool) = test_state(&workspace.database(), &workspace.data()).await;
    let (_, device) = seed_account(&pool, "blobs/operation-restart", "operation-restart").await;
    let token = "operation-restart-token";
    operation_token(&pool, device, token).await;
    let app = test_router(state.clone()).await;
    let operation = Uuid::new_v4();
    let bytes = b"restart bytes";
    let manifest = operation_manifest(bytes);
    let first = json_body(
        send(
            &app,
            operation_request(operation, manifest.clone(), token),
            StatusCode::OK,
        )
        .await,
    )
    .await;
    assert_eq!(first["missing_parts"], json!([0, 1]));
    operation_part(&app, operation, 0, &bytes[..bytes.len() / 2], token).await;
    drop(app);
    close_test_state(state, pool).await;
    let (state, pool) = test_state(&workspace.database(), &workspace.data()).await;
    let app = test_router(state.clone()).await;
    let resumed = json_body(
        send(
            &app,
            operation_request(operation, manifest.clone(), token),
            StatusCode::OK,
        )
        .await,
    )
    .await;
    assert_eq!(resumed["upload_id"], operation.to_string());
    assert_eq!(resumed["missing_parts"], json!([1]));
    operation_part(&app, operation, 1, &bytes[bytes.len() / 2..], token).await;
    let completed = operation_complete(&app, operation, token).await;
    let replay = json_body(
        send(
            &app,
            operation_request(operation, manifest.clone(), token),
            StatusCode::OK,
        )
        .await,
    )
    .await;
    assert_eq!(replay["disposition"], "complete");
    assert_eq!(replay["resource_id"], completed["resource_id"]);
    // A deduplicated first response can also be lost. Concurrent retries must
    // share one durable operation rather than appending duplicate receipts.
    let dedup = Uuid::new_v4();
    let (one, two) = tokio::join!(
        send(
            &app,
            operation_request(dedup, manifest.clone(), token),
            StatusCode::OK
        ),
        send(
            &app,
            operation_request(dedup, manifest.clone(), token),
            StatusCode::OK
        )
    );
    assert_eq!(json_body(one).await, json_body(two).await);
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM uploads WHERE id=?")
        .bind(dedup)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 1);
    let audit_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM audit_events WHERE action='resource.deduplicate'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(audit_count, 1);
    drop(app);
    close_test_state(state, pool).await;
    let (state, pool) = test_state(&workspace.database(), &workspace.data()).await;
    let app = test_router(state.clone()).await;
    let replay = json_body(
        send(
            &app,
            operation_request(dedup, manifest, token),
            StatusCode::OK,
        )
        .await,
    )
    .await;
    assert_eq!(replay["resource_id"], completed["resource_id"]);
    let audit_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM audit_events WHERE action='resource.deduplicate'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(audit_count, 1);
    drop(app);
    close_test_state(state, pool).await;
}

#[tokio::test]
async fn create_operation_validates_identity_scope_and_does_not_coalesce_new_generations() {
    let workspace = TestWorkspace::new();
    let (state, pool) = test_state(&workspace.database(), &workspace.data()).await;
    let (_, device) = seed_account(&pool, "blobs/operation-scope", "operation-scope").await;
    let (_, foreign_device) =
        seed_account(&pool, "blobs/foreign-operation", "foreign-operation").await;
    operation_token(&pool, device, "operation-owner").await;
    operation_token(&pool, foreign_device, "operation-foreign").await;
    let app = test_router(state.clone()).await;
    let manifest = operation_manifest(b"same bytes");
    for header in [
        None,
        Some("not-a-uuid"),
        Some("00000000-0000-0000-0000-000000000000"),
    ] {
        let mut request = json_request(
            Method::POST,
            "/v1/uploads",
            manifest.clone(),
            Some("operation-owner"),
            None,
        );
        request.headers_mut().remove("x-upload-operation");
        if let Some(header) = header {
            request
                .headers_mut()
                .insert("x-upload-operation", header.parse().unwrap());
        }
        send(&app, request, StatusCode::BAD_REQUEST).await;
    }
    let first = Uuid::new_v4();
    let (one, two) = tokio::join!(
        send(
            &app,
            operation_request(first, manifest.clone(), "operation-owner"),
            StatusCode::OK
        ),
        send(
            &app,
            operation_request(first, manifest.clone(), "operation-owner"),
            StatusCode::OK
        )
    );
    assert_eq!(json_body(one).await, json_body(two).await);
    send(
        &app,
        operation_request(first, manifest.clone(), "operation-foreign"),
        StatusCode::CONFLICT,
    )
    .await;
    let second = Uuid::new_v4();
    let mut changed = manifest.clone();
    changed["filename"] = json!("new metadata.jpg");
    let new = json_body(
        send(
            &app,
            operation_request(second, changed.clone(), "operation-owner"),
            StatusCode::OK,
        )
        .await,
    )
    .await;
    assert_eq!(new["upload_id"], second.to_string());
    assert_ne!(new["upload_id"], first.to_string());
    let saved: Value = sqlx::query_scalar("SELECT request FROM uploads WHERE id=?")
        .bind(second)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(saved, changed);
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM uploads")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 2);
    drop(app);
    close_test_state(state, pool).await;
}

#[tokio::test]
async fn committed_create_replay_rejects_missing_and_corrupt_blob_bytes() {
    for missing in [true, false] {
        let workspace = TestWorkspace::new();
        let (state, pool) = test_state(&workspace.database(), &workspace.data()).await;
        let (_, device) = seed_account(&pool, "blobs/replay-integrity", "replay-integrity").await;
        let token = "replay-integrity-token";
        operation_token(&pool, device, token).await;
        let app = test_router(state.clone()).await;
        let operation = Uuid::new_v4();
        operation_upload(&app, operation, b"original bytes", token).await;
        let key: String = sqlx::query_scalar("SELECT commit_final_key FROM uploads WHERE id=?")
            .bind(operation)
            .fetch_one(&pool)
            .await
            .unwrap();
        if missing {
            std::fs::remove_file(workspace.data().join(key)).unwrap();
        } else {
            std::fs::write(workspace.data().join(key), b"modified bytes").unwrap();
        }
        send(
            &app,
            operation_request(operation, operation_manifest(b"original bytes"), token),
            StatusCode::CONFLICT,
        )
        .await;
        drop(app);
        close_test_state(state, pool).await;
    }
}

#[tokio::test]
async fn committed_create_replay_rechecks_supersession_after_unlocked_validation() {
    let workspace = TestWorkspace::new();
    let (state, pool) = test_state(&workspace.database(), &workspace.data()).await;
    let (account, device) = seed_account(&pool, "blobs/replay-race", "replay-race").await;
    let token = "replay-race-token";
    operation_token(&pool, device, token).await;
    let app = test_router(state.clone()).await;
    let operation = Uuid::new_v4();
    operation_upload(&app, operation, b"old bytes", token).await;
    // New content exists for an unrelated resource, so the later operation can
    // deduplicate while the old operation holds completion admission.
    let unrelated = seed_received_upload(&state, account, device, b"new bytes", "unrelated").await;
    upload_commit::complete(&state, unrelated, account)
        .await
        .unwrap();
    let guard = state.storage.lock_upload_commit(operation).await.unwrap();
    let caller_app = app.clone();
    let caller = tokio::spawn(async move {
        send(
            &caller_app,
            operation_request(operation, operation_manifest(b"old bytes"), token),
            StatusCode::CONFLICT,
        )
        .await
    });
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        loop {
            match state.completion_admission.try_acquire(device) {
                Ok(permit) => drop(permit),
                Err(_) => break,
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    let newer = json_body(
        send(
            &app,
            operation_request(Uuid::new_v4(), operation_manifest(b"new bytes"), token),
            StatusCode::OK,
        )
        .await,
    )
    .await;
    assert_eq!(newer["disposition"], "complete");
    drop(guard);
    assert_eq!(
        json_body(caller.await.unwrap()).await["code"],
        "upload_superseded"
    );
    drop(app);
    close_test_state(state, pool).await;
}
