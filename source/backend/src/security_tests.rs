//! Regresiones de los límites de cuenta, propietario y almacenamiento.
use std::sync::Arc;
use axum::{body::{Body, to_bytes}, http::{Request, StatusCode}};
use chrono::Utc;
use serde_json::{json, Value};
use tower::ServiceExt;
use crate::{auth, config::Config, db, routes, state::AppState};

pub(crate) async fn fixture() -> (AppState, String) {
    let pool = sqlx::sqlite::SqlitePoolOptions::new().max_connections(1)
        .connect("sqlite::memory:").await.unwrap();
    sqlx::raw_sql(include_str!("db/schema.sql")).execute(&pool).await.unwrap();
    for (id, role) in [(1, "admin"), (2, "worker")] {
        sqlx::query("INSERT INTO users(id,username,display_name,password_hash,role,created_at,updated_at) VALUES(?,?,?,?,?,?,?)")
            .bind(id).bind(format!("dummy-{id}")).bind(format!("Dummy {id}"))
            .bind(auth::password::hash("dummy-password").unwrap()).bind(role)
            .bind(Utc::now()).bind(Utc::now()).execute(&pool).await.unwrap();
    }
    let root = std::env::temp_dir().join(uuid::Uuid::new_v4().to_string());
    let config = Config { port: 0, data_dir: root, public_base_url: None,
        allow_insecure_public_url: false, web_dir: "/source/web".into() };
    let (map_queue, _) = tokio::sync::mpsc::unbounded_channel();
    let state = AppState { pool, config: Arc::new(config), http: reqwest::Client::new(),
        login_limiter: Arc::default(), map_queue, public_url: Arc::default(), app_update: Arc::default() };
    let (token, _) = auth::create_session(&state.pool, 1).await.unwrap();
    (state, token)
}

async fn call(state: &AppState, token: &str, method: &str, path: &str, data: Value) -> (StatusCode, Value) {
    let response = routes::router(state.clone()).oneshot(Request::builder().method(method).uri(path)
        .header("Authorization", format!("Bearer {token}"))
        .header("Content-Type", "application/json").body(Body::from(data.to_string())).unwrap()).await.unwrap();
    let status = response.status();
    let bytes = to_bytes(response.into_body(), 1_000_000).await.unwrap();
    (status, serde_json::from_slice(&bytes).unwrap_or(Value::Null))
}

#[tokio::test]
async fn enabled_totp_cannot_be_replaced() {
    let (state, token) = fixture().await;
    sqlx::query("UPDATE users SET totp_enabled=1,totp_secret='DUMMY' WHERE id=1")
        .execute(&state.pool).await.unwrap();
    let (status, _) = call(&state, &token, "POST", "/api/me/2fa/iniciar", json!({})).await;
    if status == StatusCode::OK {
        let pending: String = sqlx::query_scalar("SELECT totp_pending_secret FROM users WHERE id=1")
            .fetch_one(&state.pool).await.unwrap();
        use hmac::{Hmac, Mac};
        let key = data_encoding::BASE32_NOPAD.decode(pending.as_bytes()).unwrap();
        let mut mac = Hmac::<sha1::Sha1>::new_from_slice(&key).unwrap();
        mac.update(&((Utc::now().timestamp() as u64 / 30).to_be_bytes()));
        let h = mac.finalize().into_bytes();
        let offset = (h[19] & 15) as usize;
        let number = u32::from_be_bytes(h[offset..offset+4].try_into().unwrap()) & 0x7fff_ffff;
        let code = format!("{:06}", number % 1_000_000);
        let (activated, _) = call(&state, &token, "POST", "/api/me/2fa/activar", json!({"codigo":code})).await;
        let stored: String = sqlx::query_scalar("SELECT totp_secret FROM users WHERE id=1")
            .fetch_one(&state.pool).await.unwrap();
        println!("dummy replacement status={activated}; active factor replaced={}", stored == pending);
    }
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let pending: Option<String> = sqlx::query_scalar("SELECT totp_pending_secret FROM users WHERE id=1")
        .fetch_one(&state.pool).await.unwrap();
    assert!(pending.is_none());
}

#[tokio::test]
async fn password_change_revokes_sessions_and_qr() {
    let (state, token) = fixture().await;
    let (old, _) = auth::create_session(&state.pool, 1).await.unwrap();
    sqlx::query("INSERT INTO qr_logins(token_hash,user_id,created_at,expires_at) VALUES(?,1,?,?)")
        .bind(auth::hash_token("dummy-qr")).bind(Utc::now())
        .bind(Utc::now()+chrono::Duration::minutes(10)).execute(&state.pool).await.unwrap();
    let (status, _) = call(&state, &token, "PUT", "/api/me/password", json!({"actual":"dummy-password","nueva":"replacement-password"})).await;
    assert_eq!(status, StatusCode::OK);
    let (status, _) = call(&state, &old, "GET", "/api/auth/me", json!({})).await;
    let (qr_status, _) = call(&state, "", "POST", "/api/auth/qr", json!({"codigo":"dummy-qr"})).await;
    println!("old session status={status}; old QR redemption status={qr_status}");
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_ne!(qr_status, StatusCode::OK);
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM qr_logins WHERE user_id=1")
        .fetch_one(&state.pool).await.unwrap();
    assert_eq!(count, 0);
}

pub(crate) fn document() -> Value {
    json!({"cargador":{"nombre":"Dummy C","nif":"DUMMYC","domicilio":"A"},
        "transportista":{"nombre":"Dummy T","nif":"DUMMYT","domicilio":"B"},
        "envios":[{"origen":"A","destino":"B","mercancia":"Dummy","peso_kg":1}],
        "fecha_transporte":"2026-11-11","matricula_camion":"1234BCD"})
}

#[tokio::test]
async fn group_expansion_keeps_owner_on_latest_widget_and_detail() {
    let (state, _) = fixture().await;
    for id in [1, 2] {
        sqlx::query("INSERT INTO decas(id,numero,user_id,user_name,token,file_rel_path,file_name,url_path,public_url,datos,fecha_transporte,grupo_id,created_at,updated_at,expires_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)")
            .bind(id).bind(format!("D-{id}")).bind(id).bind("Dummy").bind(format!("token-{id}"))
            .bind(format!("{id}.pdf")).bind(format!("{id}.pdf")).bind(format!("/deca/{id}"))
            .bind(format!("https://dummy.invalid/{id}")).bind(document().to_string())
            .bind("2026-11-11").bind("shared-group-01").bind(Utc::now()).bind(Utc::now())
            .bind(Utc::now()+chrono::Duration::days(1)).execute(&state.pool).await.unwrap();
    }
    let (token, _) = auth::create_session(&state.pool, 2).await.unwrap();
    let (status, latest) = call(&state, &token, "GET", "/api/decas/ultimo", json!({})).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(latest.as_array().unwrap().len(), 1, "foreign group member returned");
    assert_eq!(latest[0]["emitido_por_id"], 2);
    let (_, widget) = call(&state, &token, "GET", "/api/widget/ultimo", json!({})).await;
    assert_eq!(widget["viajes"].as_array().unwrap().len(), 1);
}

#[test]
fn same_second_pdfs_get_different_names_and_failed_ones_are_removed() {
    let dir = std::env::temp_dir().join(uuid::Uuid::new_v4().to_string());
    let now = Utc::now();
    let a = crate::deca::storage::Location::new(1, "Dummy", "1234BCD", now);
    let b = crate::deca::storage::Location::new(2, "Dummy", "1234BCD", now);
    assert_ne!(a.rel_path(), b.rel_path(), "duplicate PDF path");
    let first = crate::deca::storage::Reserved::new(&dir, &a).unwrap();
    let path = first.path().to_path_buf();
    assert!(path.exists());
    drop(first);
    assert!(!path.exists(), "a failed emission must remove its file");
}

#[tokio::test]
async fn initial_password_only_allows_changing_it() {
    let path = std::env::temp_dir().join(format!("{}.db", uuid::Uuid::new_v4()));
    let (mut state, _) = fixture().await;
    state.pool = db::connect(&path).await.unwrap();
    let (login, response) = call(&state, "", "POST", "/api/auth/login", json!({"usuario":"Admin","contrasena":"1234"})).await;
    assert_eq!(login, StatusCode::OK);
    assert_eq!(response["usuario"]["contrasena_por_defecto"], true);
    let token = response["token"].as_str().unwrap().to_string();
    let (admin, _) = call(&state, &token, "GET", "/api/server/settings", json!({})).await;
    assert_eq!(admin, StatusCode::FORBIDDEN, "initial password must not grant admin authority");
    let (me, _) = call(&state, &token, "GET", "/api/auth/me", json!({})).await;
    assert_eq!(me, StatusCode::OK);
    let (same, _) = call(&state, &token, "PUT", "/api/me/password", json!({"actual":"1234","nueva":"1234"})).await;
    assert_eq!(same, StatusCode::BAD_REQUEST);
    let (changed, _) = call(&state, &token, "PUT", "/api/me/password", json!({"actual":"1234","nueva":"nueva-clave"})).await;
    assert_eq!(changed, StatusCode::OK);
    let (admin, _) = call(&state, &token, "GET", "/api/server/settings", json!({})).await;
    assert_eq!(admin, StatusCode::OK);
}

#[tokio::test]
async fn worker_forwarding_headers_do_not_configure_public_origin() {
    let (state, _) = fixture().await;
    let (token, _) = auth::create_session(&state.pool, 2).await.unwrap();
    let response = routes::router(state.clone()).oneshot(Request::builder().uri("/api/setup/status")
        .header("Authorization", format!("Bearer {token}"))
        .header("x-forwarded-proto", "https").header("x-forwarded-host", "dummy.invalid")
        .body(Body::empty()).unwrap()).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert!(state.public_url.last("https://dummy.invalid").is_none(), "a worker request triggered detection");
    assert!(db::settings::load(&state.pool).await.unwrap().public_base_url_detectada.is_none());
}

#[tokio::test]
async fn restored_initial_password_is_confined() {
    let (state, _) = fixture().await;
    let mut backup = crate::backup::export_json(&state.pool).await.unwrap();
    backup["tablas"]["users"][0]["password_hash"] = auth::password::hash(db::DEFAULT_ADMIN_PASSWORD).unwrap().into();
    crate::backup::import_json(&state.pool, &backup).await.unwrap();
    let (token, _) = auth::create_session(&state.pool, 1).await.unwrap();
    let (status, _) = call(&state, &token, "GET", "/api/server/settings", json!({})).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn delayed_password_login_cannot_outlive_password_reset() {
    let (state, _) = fixture().await;
    let old_hash: String = sqlx::query_scalar("SELECT password_hash FROM users WHERE id=1")
        .fetch_one(&state.pool).await.unwrap();
    auth::update_password(&state.pool, 1, &auth::password::hash("reset-password").unwrap(), None, None).await.unwrap();
    let result = auth::issue_session(&mut *state.pool.acquire().await.unwrap(), 1,
        Some(&old_hash), false, None).await;
    assert!(result.is_err(), "previously verified password issued a session after reset");
}

#[tokio::test]
async fn current_password_and_single_use_qr_still_login() {
    let (state, _) = fixture().await;
    let (status, _) = call(&state, "", "POST", "/api/auth/login",
        json!({"usuario":"dummy-1","contrasena":"dummy-password"})).await;
    assert_eq!(status, StatusCode::OK);
    sqlx::query("INSERT INTO qr_logins(token_hash,user_id,created_at,expires_at) VALUES(?,2,?,?)")
        .bind(auth::hash_token("single-use-dummy")).bind(Utc::now())
        .bind(Utc::now()+chrono::Duration::minutes(10)).execute(&state.pool).await.unwrap();
    let (first, _) = call(&state, "", "POST", "/api/auth/qr", json!({"codigo":"single-use-dummy"})).await;
    let (second, _) = call(&state, "", "POST", "/api/auth/qr", json!({"codigo":"single-use-dummy"})).await;
    assert_eq!(first, StatusCode::OK);
    assert_ne!(second, StatusCode::OK);
}

#[tokio::test]
async fn invented_usernames_do_not_block_registered_account_admission() {
    let (state, _) = fixture().await;
    for id in 0..4100 {
        let (status, _) = call(&state, "", "POST", "/api/auth/login",
            json!({"usuario":format!("not-a-user-{id}"),"contrasena":"dummy"})).await;
        assert!(status == StatusCode::UNAUTHORIZED || status == StatusCode::TOO_MANY_REQUESTS);
    }
    let (status, _) = call(&state, "", "POST", "/api/auth/login",
        json!({"usuario":"dummy-1","contrasena":"dummy-password"})).await;
    assert_eq!(status, StatusCode::OK);
}
