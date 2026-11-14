//! Actualizaciones de la app Android: los móviles preguntan aquí si hay versión nueva
//! (ver `app_update.rs` y, en la app, `Updater.java`).

use axum::{
    extract::{Query, State},
    routing::get,
    Json, Router,
};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::auth::CurrentUser;
use crate::db::settings;
use crate::error::AppResult;
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new().route("/update", get(update))
}

#[derive(Deserialize)]
struct UpdateQuery {
    /// Solo administrador: consultar el origen ahora, sin usar la caché (botón «Comprobar»).
    #[serde(default)]
    comprobar: bool,
}

/// `{ automatico, origen, predeterminado, ultima: {version, apk, sha256, notas} | null, error }`
async fn update(State(state): State<AppState>, user: CurrentUser, Query(q): Query<UpdateQuery>) -> AppResult<Json<Value>> {
    let s = settings::load(&state.pool).await?;
    let origen = s.android_updates_source().to_string();
    let fresh = q.comprobar && user.is_admin();
    let (ultima, error) = match state.app_update.latest(&state.http, &origen, fresh).await {
        Ok(r) => (Some(r), None),
        Err(e) => (None, Some(e)),
    };
    Ok(Json(json!({
        "automatico": s.android_updates_auto,
        "origen": origen,
        "predeterminado": s.android_updates_url.is_none(),
        "ultima": ultima,
        "error": error,
    })))
}
