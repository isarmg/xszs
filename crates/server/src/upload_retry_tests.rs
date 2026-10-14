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
