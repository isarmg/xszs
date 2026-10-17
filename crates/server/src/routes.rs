use axum::{
    Json, Router,
    body::Body,
    extract::{DefaultBodyLimit, Extension, State},
    http::{HeaderMap, StatusCode},
    middleware,
    routing::{get, post, put},
};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use rand::{TryRng, rngs::SysRng};
use serde_json::Value;
use sha2::{Digest, Sha256};
use sqlx::{Row, SqlitePool};
use std::sync::Arc;
use tokio::{
    sync::{OwnedSemaphorePermit, Semaphore},
    time::timeout,
};
use uuid::Uuid;
use xcss::server_cli::{ContractJson, ContractPath};
use xszs_protocol::{
    API_BASE_PATH, BootstrapRequest, BootstrapResponse, CompleteUploadResponse,
    CreateUploadRequest, CreateUploadResponse, EmptyRequest, MediaKind, ResourceManifest,
    StorageEncoding, UPLOAD_OPERATION_HEADER, UploadDisposition, UploadPartSpec,
    UploadStatusResponse, parse_upload_operation,
};

use crate::{
    admin, api_access, audit,
    auth::{AuthContext, require_auth},
    config::Config,
    error::AppError,
    library,
    login_admission::LoginAdmission,
    metrics,
    storage::{LocalStorage, ObjectState},
    upload_commit,
};

// A legal 4096-part manifest plus bounded metadata fits this budget.
const UPLOAD_MANIFEST_BODY_LIMIT: usize = 1024 * 1024;

#[derive(Clone)]
pub struct AppState {
    pub web_directory: Option<Arc<xcss::web_assets::DirectoryAssets>>,
    pub secrets: crate::crypto::SecretBox,
    pub pool: SqlitePool,
    pub storage: LocalStorage,
    pub config: Config,
    pub login_admission: LoginAdmission,
    pub upload_admission: UploadAdmission,
    pub completion_admission: UploadAdmission,
    pub administrator:
        Arc<xcss::admin_core::AdministratorService<xcss::admin_sqlite::SqliteAdministratorStore>>,
    pub administrator_origin: xcss::admin_auth::AdministratorOriginMode,
}

#[derive(Clone)]
pub struct UploadAdmission {
    global: std::sync::Arc<Semaphore>,
    per_instance_limit: usize,
    instances: std::sync::Arc<
        std::sync::Mutex<std::collections::HashMap<Uuid, std::sync::Weak<Semaphore>>>,
    >,
}

pub(crate) struct UploadPermit {
    _global: OwnedSemaphorePermit,
    _instance: OwnedSemaphorePermit,
}

impl UploadAdmission {
    pub fn new(global_limit: usize, per_instance_limit: usize) -> Self {
        Self {
            global: std::sync::Arc::new(Semaphore::new(global_limit)),
            per_instance_limit,
            instances: std::sync::Arc::new(std::sync::Mutex::new(std::collections::HashMap::new())),
        }
    }

    pub(crate) fn try_acquire(&self, instance_id: Uuid) -> Result<UploadPermit, AppError> {
        let global = self.global.clone().try_acquire_owned().map_err(|_| {
            AppError::too_many_requests_with_message(1, "completion capacity is busy")
        })?;
        let instance = {
            let mut instances = self
                .instances
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            instances.retain(|_, semaphore| semaphore.strong_count() > 0);
            match instances
                .get(&instance_id)
                .and_then(std::sync::Weak::upgrade)
            {
                Some(semaphore) => semaphore,
                None => {
                    let semaphore = Arc::new(Semaphore::new(self.per_instance_limit));
                    instances.insert(instance_id, Arc::downgrade(&semaphore));
                    semaphore
                }
            }
        };
        let instance = instance.try_acquire_owned().map_err(|_| {
            AppError::too_many_requests_with_message(1, "instance completion capacity is busy")
        })?;
        Ok(UploadPermit {
            _global: global,
            _instance: instance,
        })
    }

    async fn acquire(&self, instance_id: Uuid) -> Result<UploadPermit, AppError> {
        let instance = {
            let mut instances = self
                .instances
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            instances.retain(|_, semaphore| semaphore.strong_count() > 0);
            if let Some(semaphore) = instances
                .get(&instance_id)
                .and_then(std::sync::Weak::upgrade)
            {
                semaphore
            } else {
                let semaphore = std::sync::Arc::new(Semaphore::new(self.per_instance_limit));
                instances.insert(instance_id, std::sync::Arc::downgrade(&semaphore));
                semaphore
            }
        };
        let wait = async {
            // Waiting for one instance must leave other instances free to perform I/O.
            let instance = instance.acquire_owned().await;
            let global = self.global.clone().acquire_owned().await;
            match (global, instance) {
                (Ok(global), Ok(instance)) => Ok(UploadPermit {
                    _global: global,
                    _instance: instance,
                }),
                _ => Err(AppError::new(
                    StatusCode::SERVICE_UNAVAILABLE,
                    "upload admission is unavailable",
                )),
            }
        };
        timeout(std::time::Duration::from_secs(30), wait)
            .await
            .map_err(|_| AppError::too_many_requests_with_message(5, "upload capacity is busy"))?
    }
}

pub(crate) async fn run_completion<T: Send + 'static>(
    permit: UploadPermit,
    work: impl std::future::Future<Output = Result<T, AppError>> + Send + 'static,
) -> Result<T, AppError> {
    tokio::spawn(async move {
        let _permit = permit;
        work.await
    })
    .await
    .map_err(|error| {
        AppError::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("upload completion task failed: {error}"),
        )
    })?
}

pub fn router(
    state: AppState,
    runtime: xcss::server_runtime::RuntimeHandle,
) -> Result<Router, AppError> {
    const JSON_BODY_LIMIT: usize = 256 * 1024;
    let max_part_bytes = state.config.max_part_bytes;
    let protected = Router::new()
        .route(
            "/uploads",
            post(create_upload).layer(DefaultBodyLimit::max(UPLOAD_MANIFEST_BODY_LIMIT)),
        )
        .route("/uploads/{id}", get(upload_status))
        .route(
            "/uploads/{id}/parts/{index}",
            put(put_part).layer(DefaultBodyLimit::max(max_part_bytes)),
        )
        .route("/uploads/{id}/complete", post(complete_upload))
        .route("/resources", get(list_resources))
        .route("/resources/{id}", get(resource_manifest))
        .route(
            "/resources/{id}/content",
            get(crate::media_delivery::content),
        )
        .route(
            "/resources/{id}/preview",
            get(crate::media_delivery::preview),
        )
        .route("/sync/head", get(library::sync_head))
        .route("/library/snapshot", get(library::snapshot))
        .route("/devices", get(library::devices))
        .route("/timeline", get(library::timeline))
        .route("/sync", get(library::sync_changes))
        .route(
            "/assets/{id}",
            get(library::get_asset)
                .patch(library::update_asset)
                .delete(library::delete_asset_permanently),
        )
        .route("/assets/{id}/trash", post(library::trash_asset))
        .route("/assets/{id}/restore", post(library::restore_asset))
        .route(
            "/albums",
            get(library::list_albums).post(library::sync_album),
        )
        .route("/tags", get(library::list_tags).post(library::create_tag))
        .route("/tags/{id}/assets", put(library::set_tag_assets))
        .route(
            "/tags/{tag_id}/assets/{asset_id}",
            post(library::add_tag_asset).delete(library::remove_tag_asset),
        )
        .route("/duplicates", get(library::duplicate_groups))
        .route(
            "/api-keys",
            get(api_access::list_api_keys).post(api_access::create_api_key),
        )
        .route(
            "/api-keys/{id}",
            axum::routing::delete(api_access::revoke_api_key),
        )
        .route("/audit-events", get(api_access::audit_events))
        .route_layer(middleware::from_fn_with_state(state.clone(), require_auth));

    let admin_protected = Router::new()
        .route("/api/v1/admin/overview", get(admin::overview))
        .route("/api/v1/admin/logs", get(admin::logs))
        .route(
            "/api/v1/admin/users/{id}",
            get(admin::get_user).put(admin::update_user),
        )
        .route("/api/v1/admin/instances", post(admin::create_instance))
        .route(
            "/api/v1/admin/instances/{id}/authorization",
            put(admin::rotate_instance_authorization),
        )
        .route(
            "/api/v1/admin/instances/{id}/revoke",
            post(admin::revoke_instance),
        )
        .route(
            "/api/v1/admin/instances/{id}",
            axum::routing::delete(admin::remove_instance),
        )
        .route_layer(middleware::from_fn_with_state(
            state.clone(),
            admin::require_admin,
        ));

    let mobile_api = Router::new()
        .route(
            "/auth/bootstrap",
            post(bootstrap).layer(DefaultBodyLimit::max(
                crate::login_admission::LOGIN_BODY_LIMIT_BYTES,
            )),
        )
        .merge(protected);

    let sensitive = Router::new()
        .route("/metrics", get(metrics::prometheus))
        .route("/admin", get(admin::web_page))
        .route("/admin/", get(admin::web_page))
        .route("/admin/assets/{*path}", get(admin::web_page))
        .nest(API_BASE_PATH, mobile_api)
        .merge(admin_protected);

    let platform = xcss::server_runtime::platform_router(
        runtime,
        "xszs",
        state.administrator_origin,
        Arc::clone(&state.administrator),
    )
    .map_err(|error| AppError::new(StatusCode::INTERNAL_SERVER_ERROR, error.to_string()))?;
    let product = sensitive
        .layer(DefaultBodyLimit::max(JSON_BODY_LIMIT))
        .with_state(state);
    Ok(Router::new()
        .merge(platform)
        .merge(product)
        .fallback(|| async { AppError::not_found("Not Found") })
        .method_not_allowed_fallback(|| async {
            AppError::new(StatusCode::METHOD_NOT_ALLOWED, "Method Not Allowed")
        })
        .layer(axum::middleware::from_fn(log_request))
        .layer(axum::middleware::from_fn(
            xcss::server_cli::request_context_middleware,
        )))
}

async fn bootstrap(
    State(state): State<AppState>,
    ContractJson(request): ContractJson<BootstrapRequest>,
) -> Result<Json<BootstrapResponse>, AppError> {
    let authorization_code = request.authorization_code.trim();
    if !valid_pairing_authorization_code(authorization_code)
        || request.device_name.trim().is_empty()
        || request.platform.trim().is_empty()
    {
        return Err(AppError::bad_request(
            "authorization_code, device_name and platform are required",
        ));
    }
    let _work = state.login_admission.acquire_work()?;
    let authorization_hash = Sha256::digest(authorization_code.as_bytes()).to_vec();
    let instance: Option<(Uuid, Uuid)> = sqlx::query_as(
        "SELECT id,account_id FROM devices \
         WHERE authorization_code_hash=? AND pairing_status='pending' AND token_hash IS NULL",
    )
    .bind(&authorization_hash)
    .fetch_optional(&state.pool)
    .await?;
    let Some((device_id, account_id)) = instance else {
        return Err(AppError::unauthorized());
    };
    state.login_admission.check_instance(device_id)?;
    let mut transaction = state.pool.begin_with("BEGIN IMMEDIATE").await?;
    let mut token_bytes = [0_u8; 32];
    SysRng
        .try_fill_bytes(&mut token_bytes)
        .expect("operating system entropy is available");
    let bearer_token = URL_SAFE_NO_PAD.encode(token_bytes);
    let token_hash = Sha256::digest(bearer_token.as_bytes()).to_vec();
    let changed = sqlx::query(
        "UPDATE devices SET platform=?,token_hash=?,pairing_status='paired',last_seen_at=datetime('now') \
         WHERE id=? AND authorization_code_hash=? AND pairing_status='pending' AND token_hash IS NULL",
    )
    .bind(request.platform.trim()).bind(token_hash).bind(device_id).bind(authorization_hash)
    .execute(&mut *transaction).await?;
    if changed.rows_affected() != 1 {
        return Err(AppError::conflict("instance was paired concurrently"));
    }
    sqlx::query(
        r#"
        INSERT INTO audit_events(
            account_id, actor_kind, actor_id, action, entity_kind, entity_id, occurred_at
        )
        VALUES (?, 'device', ?, 'device.bootstrap', 'device', ?, datetime('now'))
        "#,
    )
    .bind(account_id)
    .bind(device_id)
    .bind(device_id)
    .execute(&mut *transaction)
    .await?;
    transaction.commit().await?;
    tracing::info!(event = "xszs.instance.paired", instance_id = %device_id, instance_type = "device");
    Ok(Json(BootstrapResponse {
        account_id,
        device_id,
        bearer_token,
    }))
}

fn valid_pairing_authorization_code(value: &str) -> bool {
    value.len() == 36
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || byte.is_ascii_lowercase())
}

async fn create_upload(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthContext>,
    headers: HeaderMap,
    ContractJson(request): ContractJson<CreateUploadRequest>,
) -> Result<Json<CreateUploadResponse>, AppError> {
    let operation_id = headers
        .get(UPLOAD_OPERATION_HEADER)
        .and_then(|value| value.to_str().ok())
        .and_then(parse_upload_operation)
        .ok_or_else(|| AppError::bad_request("X-Upload-Operation must be a non-nil UUID"))?;
    validate_upload_request(&request, state.config.max_part_bytes)?;
    let mut transaction = state.pool.begin_with("BEGIN IMMEDIATE").await?;
    if let Some(response) =
        replay_upload_operation(&mut transaction, operation_id, &auth, &request).await?
    {
        transaction.commit().await?;
        return verified_upload_replay(&state, operation_id, &auth, &request, response).await;
    }
    let policy = account_policy_for_update(&mut transaction, auth.account_id).await?;
    let existing_blob = sqlx::query(
        "SELECT id, storage_path, stored_size FROM blobs WHERE account_id = ? AND content_blake3 = ?",
    )
    .bind(auth.account_id)
    .bind(&request.content_blake3)
    .fetch_optional(&mut *transaction)
    .await?;
    if let Some(existing_blob) = existing_blob {
        let blob_id: Uuid = existing_blob.get("id");
        let storage_path: String = existing_blob.get("storage_path");
        let stored_size: i64 = existing_blob.get("stored_size");
        transaction.rollback().await?;
        let size_matches = u64::try_from(stored_size).ok() == Some(request.content_size);
        let file_matches = size_matches
            && state
                .storage
                .inspect_object(
                    &policy.storage_path,
                    &storage_path,
                    request.content_size,
                    &request.content_blake3,
                )
                .await?
                == ObjectState::Matches;
        if !file_matches {
            return Err(AppError::conflict(
                "deduplicated blob is missing or does not match its full hash",
            ));
        }
        let mut transaction = state.pool.begin_with("BEGIN IMMEDIATE").await?;
        // Hashing releases the write lock. A concurrent retry may have created
        // this operation while the object was being verified.
        if let Some(response) =
            replay_upload_operation(&mut transaction, operation_id, &auth, &request).await?
        {
            transaction.commit().await?;
            return verified_upload_replay(&state, operation_id, &auth, &request, response).await;
        }
        let blob_still_exists: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM blobs WHERE id = ? AND account_id = ? \
             AND content_blake3 = ? AND storage_path = ? AND stored_size = ?)",
        )
        .bind(blob_id)
        .bind(auth.account_id)
        .bind(&request.content_blake3)
        .bind(&storage_path)
        .bind(stored_size)
        .fetch_one(&mut *transaction)
        .await?;
        if !blob_still_exists {
            return Err(AppError::conflict(
                "deduplicated blob changed during verification",
            ));
        }
        let asset_id = upsert_asset(&mut transaction, &auth, &request).await?;
        let resource_id = upsert_resource(&mut transaction, asset_id, blob_id, &request).await?;
        // Deduplication is a completed backup too. Record it in the same ordered
        // receipt ledger as uploaded resources so an older in-flight upload
        // cannot overwrite this version when it eventually completes.
        let receipt_id = operation_id;
        sqlx::query(
            r#"INSERT INTO uploads(
                 id,account_id,device_id,asset_id,source_resource_id,content_blake3,request,
                 state,commit_state,commit_final_key,commit_account_path,commit_expected_size,
                 commit_expected_blake3,commit_blob_id,commit_resource_id,commit_deduplicated,
                 created_at,updated_at,completed_at)
               VALUES(?,?,?,?,?,?,?,'complete','committed',?,?,?,?,?,?,1,
                      datetime('now'),datetime('now'),datetime('now'))"#,
        )
        .bind(receipt_id)
        .bind(auth.account_id)
        .bind(auth.device_id)
        .bind(asset_id)
        .bind(&request.source_resource_id)
        .bind(&request.content_blake3)
        .bind(serde_json::to_value(&request)?)
        .bind(&storage_path)
        .bind(&policy.storage_path)
        .bind(stored_size)
        .bind(&request.content_blake3)
        .bind(blob_id)
        .bind(resource_id)
        .execute(&mut *transaction)
        .await?;
        audit::record_change(
            &mut transaction,
            auth.account_id,
            "asset",
            asset_id,
            "upsert",
        )
        .await?;
        audit::record_in_transaction(
            &mut transaction,
            &auth,
            "resource.deduplicate",
            Some("resource"),
            Some(resource_id),
        )
        .await?;
        transaction.commit().await?;
        return Ok(Json(CreateUploadResponse {
            disposition: UploadDisposition::Complete,
            upload_id: None,
            resource_id: Some(resource_id),
            missing_parts: Vec::new(),
        }));
    }

    let asset_id = upsert_asset(&mut transaction, &auth, &request).await?;

    ensure_quota(
        &mut transaction,
        auth.account_id,
        policy.quota_bytes,
        request_storage_size(&request)?,
    )
    .await?;

    let request_json = serde_json::to_value(&request)?;
    let upload_id: Uuid = sqlx::query_scalar(
        r#"
        INSERT INTO uploads(
            id, account_id, device_id, asset_id, source_resource_id, content_blake3, request,
            created_at, updated_at
        )
        VALUES (?, ?, ?, ?, ?, ?, ?, datetime('now'), datetime('now'))
        RETURNING id
        "#,
    )
    .bind(operation_id)
    .bind(auth.account_id)
    .bind(auth.device_id)
    .bind(asset_id)
    .bind(&request.source_resource_id)
    .bind(&request.content_blake3)
    .bind(request_json)
    .fetch_one(&mut *transaction)
    .await?;
    for part in &request.parts {
        sqlx::query(
            "INSERT INTO upload_parts(upload_id, part_index, expected_size, expected_blake3) VALUES (?, ?, ?, ?)",
        )
        .bind(upload_id)
        .bind(part.index as i32)
        .bind(part.size as i64)
        .bind(&part.blake3)
        .execute(&mut *transaction)
        .await?;
    }
    transaction.commit().await?;
    Ok(Json(CreateUploadResponse {
        disposition: UploadDisposition::Upload,
        upload_id: Some(upload_id),
        resource_id: None,
        missing_parts: request.parts.iter().map(|part| part.index).collect(),
    }))
}

// Receipt replay retains the complete endpoint's physical-object validation.
// Hash outside the SQLite write lock, then recheck ordering before reporting
// success: a newer operation may have committed while the hash was running.
async fn verified_upload_replay(
    state: &AppState,
    operation_id: Uuid,
    auth: &AuthContext,
    request: &CreateUploadRequest,
    response: CreateUploadResponse,
) -> Result<Json<CreateUploadResponse>, AppError> {
    if matches!(response.disposition, UploadDisposition::Upload) {
        return Ok(Json(response));
    }
    upload_commit::complete(state, operation_id, auth.account_id).await?;
    let mut transaction = state.pool.begin_with("BEGIN IMMEDIATE").await?;
    let response = replay_upload_operation(&mut transaction, operation_id, auth, request)
        .await?
        .ok_or_else(|| AppError::conflict("upload operation is no longer available"))?;
    transaction.commit().await?;
    Ok(Json(response))
}

// The client supplies its durable preparation generation, including when the
// first create response is lost. Content hashes cannot identify operations:
// a deliberate A -> B -> A edit is distinct from retrying the original A.
async fn replay_upload_operation(
    transaction: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    operation_id: Uuid,
    auth: &AuthContext,
    request: &CreateUploadRequest,
) -> Result<Option<CreateUploadResponse>, AppError> {
    let row = sqlx::query(
        "SELECT account_id, device_id, request, commit_state, commit_resource_id, commit_error \
         FROM uploads WHERE id = ?",
    )
    .bind(operation_id)
    .fetch_optional(&mut **transaction)
    .await?;
    let Some(row) = row else {
        return Ok(None);
    };
    if row.get::<Uuid, _>("account_id") != auth.account_id
        || row.get::<Uuid, _>("device_id") != auth.device_id
    {
        return Err(AppError::conflict("upload operation is unavailable"));
    }
    if row.get::<Value, _>("request") != serde_json::to_value(request)? {
        return Err(AppError::conflict("upload operation manifest changed"));
    }
    let superseded: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM uploads newer JOIN uploads current ON current.id = ? \
         WHERE newer.account_id = current.account_id AND newer.asset_id = current.asset_id \
         AND newer.source_resource_id = current.source_resource_id \
         AND newer.rowid > current.rowid AND newer.commit_state = 'committed')",
    )
    .bind(operation_id)
    .fetch_one(&mut **transaction)
    .await?;
    if superseded
        || row.get::<Option<String>, _>("commit_error").as_deref()
            == Some("superseded by newer committed resource")
    {
        return Err(AppError::upload_superseded());
    }
    match row.get::<String, _>("commit_state").as_str() {
        "committed" => Ok(Some(CreateUploadResponse {
            disposition: UploadDisposition::Complete,
            upload_id: None,
            resource_id: Some(row.try_get::<Uuid, _>("commit_resource_id")?),
            missing_parts: Vec::new(),
        })),
        "receiving" | "commit_started" | "finalizing" => {
            let missing: Vec<i32> = sqlx::query_scalar(
                "SELECT part_index FROM upload_parts WHERE upload_id = ? AND received_at IS NULL ORDER BY part_index",
            )
            .bind(operation_id)
            .fetch_all(&mut **transaction)
            .await?;
            Ok(Some(CreateUploadResponse {
                disposition: UploadDisposition::Upload,
                upload_id: Some(operation_id),
                resource_id: None,
                missing_parts: missing.into_iter().map(|index| index as u32).collect(),
            }))
        }
        _ => Err(AppError::conflict("upload operation cannot be resumed")),
    }
}

async fn upsert_asset(
    transaction: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    auth: &AuthContext,
    request: &CreateUploadRequest,
) -> Result<Uuid, AppError> {
    Ok(sqlx::query_scalar(
        r#"
        INSERT INTO assets(
            id, account_id, device_id, source_asset_id, media_kind, source_created_at_ms, favorite,
            created_at, updated_at
        )
        VALUES (?, ?, ?, ?, ?, ?, ?, datetime('now'), datetime('now'))
        ON CONFLICT(account_id, device_id, source_asset_id) DO UPDATE SET
            media_kind = excluded.media_kind,
            source_created_at_ms = excluded.source_created_at_ms,
            updated_at = datetime('now')
        RETURNING id
        "#,
    )
    .bind(Uuid::new_v4())
    .bind(auth.account_id)
    .bind(auth.device_id)
    .bind(&request.source_asset_id)
    .bind(request.media_kind.as_str())
    .bind(request.source_created_at_ms)
    .bind(
        request
            .metadata
            .as_ref()
            .and_then(|value| value.get("favorite"))
            .and_then(Value::as_bool)
            .unwrap_or(false),
    )
    .fetch_one(&mut **transaction)
    .await?)
}

async fn put_part(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthContext>,
    ContractPath((upload_id, index)): ContractPath<(Uuid, u32)>,
    body: Body,
) -> Result<StatusCode, AppError> {
    let row = sqlx::query(
        r#"
        SELECT p.expected_size, p.expected_blake3, u.state, u.commit_state
        FROM upload_parts p JOIN uploads u ON u.id = p.upload_id
        WHERE p.upload_id = ? AND p.part_index = ? AND u.account_id = ?
        "#,
    )
    .bind(upload_id)
    .bind(index as i32)
    .bind(auth.account_id)
    .fetch_optional(&state.pool)
    .await?
    .ok_or_else(|| AppError::not_found("upload part not found"))?;
    if row.get::<String, _>("commit_state") == "committed" {
        return Ok(StatusCode::NO_CONTENT);
    }
    if row.get::<String, _>("commit_state") != "receiving" {
        return Err(AppError::conflict(
            "upload parts are frozen after commit starts",
        ));
    }
    let spec = UploadPartSpec {
        index,
        size: row.get::<i64, _>("expected_size") as u64,
        blake3: row.get("expected_blake3"),
    };
    let _admission = state.upload_admission.acquire(auth.device_id).await?;
    state
        .storage
        .put_part(upload_id, &spec, body, state.config.max_part_bytes)
        .await?;
    sqlx::query(
        "UPDATE upload_parts SET received_size = expected_size, received_at = datetime('now') WHERE upload_id = ? AND part_index = ?",
    )
    .bind(upload_id)
    .bind(index as i32)
    .execute(&state.pool)
    .await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn upload_status(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthContext>,
    ContractPath(upload_id): ContractPath<Uuid>,
) -> Result<Json<UploadStatusResponse>, AppError> {
    let state_value: String =
        sqlx::query_scalar("SELECT commit_state FROM uploads WHERE id = ? AND account_id = ?")
            .bind(upload_id)
            .bind(auth.account_id)
            .fetch_optional(&state.pool)
            .await?
            .ok_or_else(|| AppError::not_found("upload not found"))?;
    Ok(Json(UploadStatusResponse {
        upload_id,
        state: state_value,
        missing_parts: missing_parts(&state.pool, upload_id).await?,
    }))
}

async fn complete_upload(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthContext>,
    ContractPath(upload_id): ContractPath<Uuid>,
    ContractJson(_request): ContractJson<EmptyRequest>,
) -> Result<Json<CompleteUploadResponse>, AppError> {
    let outcome = upload_commit::complete(&state, upload_id, auth.account_id).await?;
    Ok(Json(CompleteUploadResponse {
        resource_id: outcome.resource_id,
        asset_id: outcome.asset_id,
        deduplicated: outcome.deduplicated,
    }))
}

async fn list_resources(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthContext>,
) -> Result<Json<Vec<ResourceManifest>>, AppError> {
    let rows = sqlx::query(
        r#"
        SELECT r.id AS resource_id, r.asset_id, r.source_resource_id, r.role, r.filename,
               r.mime_type, r.metadata,
               a.source_asset_id, a.media_kind, a.source_created_at_ms,
               b.plaintext_size, b.content_blake3, b.part_manifest
        FROM resources r
        JOIN assets a ON a.id = r.asset_id
        JOIN blobs b ON b.id = r.blob_id
        WHERE a.account_id = ? AND a.deleted_at IS NULL
        ORDER BY a.source_created_at_ms DESC, r.created_at DESC
        LIMIT 1000
        "#,
    )
    .bind(auth.account_id)
    .fetch_all(&state.pool)
    .await?;
    let resources = rows
        .iter()
        .map(resource_manifest_from_row)
        .collect::<Result<Vec<_>, _>>()?;
    Ok(Json(resources))
}

async fn resource_manifest(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthContext>,
    ContractPath(resource_id): ContractPath<Uuid>,
) -> Result<Json<ResourceManifest>, AppError> {
    Ok(Json(
        load_manifest(&state.pool, auth.account_id, resource_id).await?,
    ))
}

fn validate_upload_request(
    request: &CreateUploadRequest,
    max_part_bytes: usize,
) -> Result<(), AppError> {
    if request.source_asset_id.is_empty()
        || request.source_resource_id.is_empty()
        || request.filename.is_empty()
        || request.mime_type.is_empty()
        || !is_blake3(&request.content_blake3)
        || request.parts.is_empty()
        || request.parts.len() > 4096
        || request.source_asset_id.len() > 4096
        || request.source_resource_id.len() > 4096
        || request.role.is_empty()
        || request.role.len() > 64
        || request.filename.len() > 1024
        || request.mime_type.len() > 255
        || request.metadata.as_ref().is_some_and(|value| {
            serde_json::to_vec(value).is_ok_and(|bytes| bytes.len() > 64 * 1024)
        })
    {
        return Err(AppError::bad_request("upload manifest is incomplete"));
    }
    for (position, part) in request.parts.iter().enumerate() {
        if part.index as usize != position
            || (part.size == 0 && request.content_size != 0)
            || part.size > max_part_bytes as u64
            || !is_blake3(&part.blake3)
        {
            return Err(AppError::bad_request("invalid part manifest"));
        }
    }
    let total = request
        .parts
        .iter()
        .try_fold(0_u64, |sum, part| sum.checked_add(part.size))
        .ok_or_else(|| AppError::bad_request("upload size overflow"))?;
    if total != request.content_size {
        return Err(AppError::bad_request(
            "part sizes do not match content_size",
        ));
    }
    Ok(())
}

fn is_blake3(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

async fn missing_parts(pool: &SqlitePool, upload_id: Uuid) -> Result<Vec<u32>, AppError> {
    let values: Vec<i32> = sqlx::query_scalar(
        "SELECT part_index FROM upload_parts WHERE upload_id = ? AND received_at IS NULL ORDER BY part_index",
    )
    .bind(upload_id)
    .fetch_all(pool)
    .await?;
    Ok(values.into_iter().map(|value| value as u32).collect())
}

async fn upsert_resource(
    transaction: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    asset_id: Uuid,
    blob_id: Uuid,
    request: &CreateUploadRequest,
) -> Result<Uuid, AppError> {
    Ok(sqlx::query_scalar(
        r#"
        INSERT INTO resources(
            id, asset_id, blob_id, source_resource_id, role, filename, mime_type,
            metadata, created_at
        ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, datetime('now'))
        ON CONFLICT(asset_id, source_resource_id) DO UPDATE SET
            blob_id = excluded.blob_id,
            role = excluded.role,
            filename = excluded.filename,
            mime_type = excluded.mime_type,
            metadata = excluded.metadata
        RETURNING id
        "#,
    )
    .bind(Uuid::new_v4())
    .bind(asset_id)
    .bind(blob_id)
    .bind(&request.source_resource_id)
    .bind(&request.role)
    .bind(&request.filename)
    .bind(&request.mime_type)
    .bind(&request.metadata)
    .fetch_one(&mut **transaction)
    .await?)
}

async fn load_manifest(
    pool: &SqlitePool,
    account_id: Uuid,
    resource_id: Uuid,
) -> Result<ResourceManifest, AppError> {
    let row = sqlx::query(
        r#"
        SELECT r.id AS resource_id, r.asset_id, r.source_resource_id, r.role, r.filename,
               r.mime_type, r.metadata,
               a.source_asset_id, a.media_kind, a.source_created_at_ms,
               b.plaintext_size, b.content_blake3, b.part_manifest
        FROM resources r
        JOIN assets a ON a.id = r.asset_id
        JOIN blobs b ON b.id = r.blob_id
        WHERE r.id = ? AND a.account_id = ?
        "#,
    )
    .bind(resource_id)
    .bind(account_id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| AppError::not_found("resource not found"))?;
    resource_manifest_from_row(&row)
}

fn resource_manifest_from_row(row: &sqlx::sqlite::SqliteRow) -> Result<ResourceManifest, AppError> {
    let resource_id: Uuid = row.get("resource_id");
    let media_kind = match row.get::<String, _>("media_kind").as_str() {
        "photo" => MediaKind::Photo,
        "video" => MediaKind::Video,
        _ => MediaKind::Other,
    };
    Ok(ResourceManifest {
        resource_id: row.get("resource_id"),
        asset_id: row.get("asset_id"),
        source_asset_id: row.get("source_asset_id"),
        source_resource_id: row.get("source_resource_id"),
        media_kind,
        role: row.get("role"),
        filename: row.get("filename"),
        mime_type: row.get("mime_type"),
        source_created_at_ms: row.get("source_created_at_ms"),
        content_size: row.get::<i64, _>("plaintext_size") as u64,
        content_blake3: row.get("content_blake3"),
        storage_encoding: StorageEncoding::PlainV1,
        metadata: row.get("metadata"),
        parts: serde_json::from_value(row.get::<Value, _>("part_manifest"))?,
        content_path: format!("{API_BASE_PATH}/resources/{resource_id}/content"),
    })
}

struct AccountPolicy {
    storage_path: String,
    quota_bytes: i64,
}

async fn account_policy_for_update(
    transaction: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    account_id: Uuid,
) -> Result<AccountPolicy, AppError> {
    let row = sqlx::query("SELECT storage_path, quota_bytes FROM accounts WHERE id = ?")
        .bind(account_id)
        .fetch_optional(&mut **transaction)
        .await?
        .ok_or_else(AppError::unauthorized)?;
    Ok(AccountPolicy {
        storage_path: row.get("storage_path"),
        quota_bytes: row.get("quota_bytes"),
    })
}

async fn ensure_quota(
    transaction: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    account_id: Uuid,
    quota_bytes: i64,
    requested_bytes: i64,
) -> Result<(), AppError> {
    if quota_bytes == 0 {
        return Ok(());
    }
    let used_bytes: i64 =
        sqlx::query_scalar("SELECT COALESCE(SUM(stored_size), 0) FROM blobs WHERE account_id = ?")
            .bind(account_id)
            .fetch_one(&mut **transaction)
            .await?;
    let reserved_bytes: i64 = sqlx::query_scalar(
        r#"
        SELECT COALESCE(SUM(p.expected_size), 0)
        FROM upload_parts p
        JOIN uploads u ON u.id = p.upload_id
        WHERE u.account_id = ? AND u.state = 'uploading'
          AND u.commit_state IN ('receiving', 'commit_started', 'finalizing', 'unknown')
        "#,
    )
    .bind(account_id)
    .fetch_one(&mut **transaction)
    .await?;
    let projected = used_bytes
        .checked_add(reserved_bytes)
        .and_then(|value| value.checked_add(requested_bytes))
        .unwrap_or(i64::MAX);
    if projected > quota_bytes {
        return Err(AppError::new(
            StatusCode::INSUFFICIENT_STORAGE,
            "account storage quota exceeded",
        ));
    }
    Ok(())
}

fn request_storage_size(request: &CreateUploadRequest) -> Result<i64, AppError> {
    let total = request
        .parts
        .iter()
        .try_fold(0_u64, |sum, part| sum.checked_add(part.size));
    let total = total.ok_or_else(|| AppError::bad_request("upload size overflow"))?;
    i64::try_from(total).map_err(|_| AppError::bad_request("upload is too large"))
}

async fn log_request(
    request: axum::extract::Request,
    next: axum::middleware::Next,
) -> axum::response::Response {
    use tracing::Instrument;
    let request_id = request
        .extensions()
        .get::<xcss::contracts::RequestId>()
        .map(|value| value.as_str().to_owned())
        .unwrap_or_default();
    let span = tracing::info_span!("http.request", request_id = request_id.as_str());
    async move {
        let started = std::time::Instant::now();
        let response = next.run(request).await;
        tracing::info!(
            event = "xszs.http.completed",
            component = "http",
            status = response.status().as_u16() as u64,
            duration_ms = started.elapsed().as_millis().min(u128::from(u64::MAX)) as u64
        );
        response
    }
    .instrument(span)
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn cancelled_completion_retains_its_instance_and_global_work_slots() {
        let admission = UploadAdmission::new(2, 1);
        let instance = Uuid::new_v4();
        let (started, running) = tokio::sync::oneshot::channel();
        let (complete, done) = tokio::sync::oneshot::channel();
        let caller = tokio::spawn(run_completion(
            admission.try_acquire(instance).unwrap(),
            async move {
                started.send(()).unwrap();
                done.await.unwrap();
                Ok(())
            },
        ));
        running.await.unwrap();
        caller.abort();
        assert!(caller.await.unwrap_err().is_cancelled());
        assert!(admission.try_acquire(instance).is_err());
        let other = admission.try_acquire(Uuid::new_v4()).unwrap();
        assert!(admission.try_acquire(Uuid::new_v4()).is_err());
        complete.send(()).unwrap();
        timeout(std::time::Duration::from_secs(1), async {
            while admission.try_acquire(instance).is_err() {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        drop(other);
        assert_eq!(admission.global.available_permits(), 2);
    }

    #[test]
    fn mobile_manifest_budget_accepts_4096_parts_and_rejects_overflow() {
        let mut request: CreateUploadRequest = serde_json::from_value(serde_json::json!({
            "source_asset_id":"asset", "source_resource_id":"resource", "media_kind":"photo",
            "role":"primary", "filename":"photo.jpg", "mime_type":"image/jpeg", "source_created_at_ms":0,
            "storage_encoding":"plain-v1", "content_size":4096, "content_blake3":"a".repeat(64),
            "metadata":{"description":"x".repeat(64 * 1024 - 32)},
            "parts":(0..4096).map(|index| serde_json::json!({"index":index,"size":1,"blake3":"b".repeat(64)})).collect::<Vec<_>>()
        })).unwrap();
        let bytes = serde_json::to_vec(&request).unwrap();
        assert!(
            bytes.len() > 64 * 1024,
            "exercise the old body-limit failure"
        );
        assert!(bytes.len() <= UPLOAD_MANIFEST_BODY_LIMIT);
        validate_upload_request(&request, 16 * 1024 * 1024).unwrap();
        request.parts.push(xszs_protocol::UploadPartSpec {
            index: 4096,
            size: 1,
            blake3: "b".repeat(64),
        });
        request.content_size += 1;
        assert_eq!(
            validate_upload_request(&request, 16 * 1024 * 1024)
                .unwrap_err()
                .status,
            StatusCode::BAD_REQUEST
        );
        request.parts.truncate(1);
        request.content_size = 16 * 1024 * 1024 + 1;
        request.parts[0].size = request.content_size;
        assert_eq!(
            validate_upload_request(&request, 16 * 1024 * 1024)
                .unwrap_err()
                .status,
            StatusCode::BAD_REQUEST
        );
    }

    #[tokio::test]
    async fn queued_instance_does_not_reserve_another_instances_upload_slot() {
        let admission = UploadAdmission::new(2, 1);
        let instance = Uuid::new_v4();
        let first = admission.acquire(instance).await.unwrap();
        {
            let waiting = admission.acquire(instance);
            tokio::pin!(waiting);
            assert!(
                timeout(std::time::Duration::from_millis(20), &mut waiting)
                    .await
                    .is_err()
            );
            assert_eq!(admission.global.available_permits(), 1);
            let other = timeout(
                std::time::Duration::from_secs(1),
                admission.acquire(Uuid::new_v4()),
            )
            .await
            .expect("an idle instance must be able to use the remaining slot")
            .unwrap();
            drop(other);
        }
        drop(first);
        assert_eq!(admission.global.available_permits(), 2);
    }

    #[tokio::test]
    async fn cancelling_a_global_wait_releases_the_instance_admission() {
        let admission = UploadAdmission::new(1, 1);
        let first = admission.acquire(Uuid::new_v4()).await.unwrap();
        let other_instance = Uuid::new_v4();
        {
            let waiting = admission.acquire(other_instance);
            tokio::pin!(waiting);
            assert!(
                timeout(std::time::Duration::from_millis(20), &mut waiting)
                    .await
                    .is_err()
            );
        }
        drop(first);
        let other = timeout(
            std::time::Duration::from_secs(1),
            admission.acquire(other_instance),
        )
        .await
        .expect("cancelled requests must release instance capacity")
        .unwrap();
        drop(other);
        assert_eq!(admission.global.available_permits(), 1);
    }
}
