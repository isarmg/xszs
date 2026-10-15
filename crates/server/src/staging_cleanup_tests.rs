use super::*;

#[cfg(target_os = "linux")]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn staging_cleanup_preserves_commit_started_after_its_reference_snapshot() {
    let workspace = TestWorkspace::new();
    let (state, pool) = test_state(&workspace.database(), &workspace.data()).await;
    let (account_id, device_id) = seed_account(&pool, "blobs/stage-race", "stage-race").await;
    let upload_id =
        seed_received_upload(&state, account_id, device_id, b"live stage", "stage-race").await;
    // An independent storage handle models another request without sharing the
    // cleaner's filesystem test hook. Both use the same real database/files.
    let mut writer = state.clone();
    writer.storage = LocalStorage::new(workspace.data()).await.unwrap();
    let (arrived_tx, arrived_rx) = tokio::sync::oneshot::channel();
    let (release_tx, release_rx) = std::sync::mpsc::channel();
    state.storage.inject_before_rooted_operation_once(move || {
        arrived_tx.send(()).unwrap();
        release_rx
            .recv_timeout(std::time::Duration::from_secs(30))
            .unwrap();
    });
    let cleaner_state = state.clone();
    let cleaner =
        tokio::spawn(async move { upload_commit::reconcile_orphan_stages(&cleaner_state).await });
    arrived_rx.await.unwrap();
    // Probe the real SQLite writer lock with no waiting. Before the fix this
    // succeeds, allowing a new durable receipt/stage into the stale snapshot.
    let mut probe = pool.acquire().await.unwrap();
    sqlx::query("PRAGMA busy_timeout=0")
        .execute(&mut *probe)
        .await
        .unwrap();
    let lock = sqlx::query("BEGIN IMMEDIATE").execute(&mut *probe).await;
    let writer_can_start = lock.is_ok();
    if writer_can_start {
        sqlx::query("ROLLBACK").execute(&mut *probe).await.unwrap();
    } else {
        let error = lock.unwrap_err();
        assert_eq!(
            error.as_database_error().unwrap().code().as_deref(),
            Some("5")
        );
    }
    sqlx::query("PRAGMA busy_timeout=5000")
        .execute(&mut *probe)
        .await
        .unwrap();
    drop(probe);
    if writer_can_start {
        upload_commit::complete_with_failpoint(
            &writer,
            upload_id,
            account_id,
            CommitFailpoint::Finalizing,
        )
        .await
        .expect_err("pause with a durable finalizing receipt and stage");
    }
    release_tx.send(()).unwrap();
    cleaner.await.unwrap().unwrap();
    if !writer_can_start {
        upload_commit::complete_with_failpoint(
            &writer,
            upload_id,
            account_id,
            CommitFailpoint::Finalizing,
        )
        .await
        .expect_err("start after cleaner releases the writer lock");
    }
    assert!(
        writer_can_start,
        "directory enumeration must not hold SQLite's writer lock"
    );
    upload_commit::complete(&writer, upload_id, account_id)
        .await
        .expect("cleanup must not discard the stage of a durable finalizing upload");
    drop(writer);
    close_test_state(state, pool).await;
}
