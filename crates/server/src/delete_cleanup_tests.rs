use super::*;

#[tokio::test]
async fn instance_deletion_preserves_pending_blob_reclamation() {
    let workspace = TestWorkspace::new();
    let (state, pool) = test_state(&workspace.database(), &workspace.data()).await;
    let (account_id, device_id) =
        seed_account(&pool, "blobs/delete-instance", "delete-instance").await;
    let content = b"pending blob reclamation";
    let upload_id =
        seed_received_upload(&state, account_id, device_id, content, "delete-instance").await;
    let committed = upload_commit::complete(&state, upload_id, account_id)
        .await
        .unwrap();
    let object_key: String =
        sqlx::query_scalar("SELECT storage_path FROM blobs WHERE account_id = ?")
            .bind(account_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    let object_path = workspace.data().join(object_key);
    sqlx::query("UPDATE assets SET deleted_at = datetime('now') WHERE id = ?")
        .bind(committed.asset_id)
        .execute(&pool)
        .await
        .unwrap();
    std::fs::remove_file(&object_path).unwrap();
    std::fs::create_dir(&object_path).unwrap();
    let status = library::delete_asset_permanently(
        axum::extract::State(state.clone()),
        axum::extract::Extension(AuthContext {
            account_id,
            device_id,
            actor_kind: "device".into(),
            actor_id: device_id,
            credential_hash: vec![],
        }),
        xcss::server_cli::ContractPath(committed.asset_id),
        xcss::server_cli::ContractJson(xszs_protocol::EmptyRequest {}),
    )
    .await
    .expect("asset deletion remains durable while cleanup is pending");
    assert_eq!(status, StatusCode::ACCEPTED);
    admin::revoke_instance(
        axum::extract::State(state.clone()),
        xcss::server_cli::ContractPath(device_id),
    )
    .await
    .expect("revoke instance");
    let error = admin::remove_instance(
        axum::extract::State(state.clone()),
        xcss::server_cli::ContractPath(device_id),
    )
    .await
    .expect_err("account deletion must retain the pending blob reclamation row");
    assert_eq!(error.status, StatusCode::CONFLICT);
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM blobs WHERE account_id = ?")
            .bind(account_id)
            .fetch_one(&pool)
            .await
            .unwrap(),
        1
    );
    std::fs::remove_dir(&object_path).unwrap();
    std::fs::write(&object_path, content).unwrap();
    let report = library::reconcile_orphan_blobs(&state, Some(account_id))
        .await
        .unwrap();
    assert_eq!(report.removed, 1);
    assert!(!object_path.exists());
    admin::remove_instance(
        axum::extract::State(state.clone()),
        xcss::server_cli::ContractPath(device_id),
    )
    .await
    .expect("delete instance after reclamation succeeds");
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM accounts WHERE id = ?")
            .bind(account_id)
            .fetch_one(&pool)
            .await
            .unwrap(),
        0
    );
    close_test_state(state, pool).await;
}

#[tokio::test]
async fn instance_revocation_retries_never_permanently_delete_accounts() {
    let workspace = TestWorkspace::new();
    let (state, pool) = test_state(&workspace.database(), &workspace.data()).await;
    for (initial, terminal) in [("pending", "cancelled"), ("paired", "revoked")] {
        let (account_id, device_id) =
            seed_account(&pool, &format!("blobs/retry-{initial}"), initial).await;
        sqlx::query("UPDATE devices SET pairing_status = ?1, token_hash = CASE WHEN ?1 = 'pending' THEN NULL ELSE token_hash END WHERE id = ?2")
            .bind(initial)
            .bind(device_id)
            .execute(&pool)
            .await
            .unwrap();
        let error = admin::remove_instance(
            axum::extract::State(state.clone()),
            xcss::server_cli::ContractPath(device_id),
        )
        .await
        .expect_err("permanent deletion must not implicitly revoke an active instance");
        assert_eq!(error.status, StatusCode::CONFLICT);
        for _ in 0..2 {
            assert_eq!(
                admin::revoke_instance(
                    axum::extract::State(state.clone()),
                    xcss::server_cli::ContractPath(device_id),
                )
                .await
                .expect("revocation and its retry succeed without deleting history"),
                StatusCode::NO_CONTENT
            );
            let status: String =
                sqlx::query_scalar("SELECT pairing_status FROM devices WHERE id = ?")
                    .bind(device_id)
                    .fetch_one(&pool)
                    .await
                    .unwrap();
            assert_eq!(status, terminal);
            assert_eq!(
                sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM accounts WHERE id = ?")
                    .bind(account_id)
                    .fetch_one(&pool)
                    .await
                    .unwrap(),
                1
            );
        }
        admin::remove_instance(
            axum::extract::State(state.clone()),
            xcss::server_cli::ContractPath(device_id),
        )
        .await
        .expect("only a distinct permanent-delete action removes the empty account");
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM accounts WHERE id = ?")
                .bind(account_id)
                .fetch_one(&pool)
                .await
                .unwrap(),
            0
        );
    }
    close_test_state(state, pool).await;
}
