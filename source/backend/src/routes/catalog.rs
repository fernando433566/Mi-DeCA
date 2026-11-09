//! Base de datos compartida. Cualquier usuario puede añadir camiones, remolques,
//! empresas (CIF/NIF), direcciones y mercancías, y todos los demás pueden usarlos.
//! Editar o borrar: quien lo creó o el administrador.
//!
//! Al emitir un DeCA, los datos nuevos se añaden automáticamente (ver [`remember`]).

use axum::{
    extract::{Path, Query, State},
    routing::get,
    Json, Router,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::FromRow;

use crate::auth::CurrentUser;
use crate::deca::model::{normalize_plate, DecaDatos};
use crate::error::{AppError, AppResult};
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/trucks", get(list_trucks).post(create_truck))
        .route("/trucks/{id}", get(get_truck).put(update_truck).delete(delete_truck))
        .route("/trailers", get(list_trailers).post(create_trailer))
        .route("/trailers/{id}", axum::routing::put(update_trailer).delete(delete_trailer))
        .route("/parties", get(list_parties).post(create_party))
        .route("/parties/{id}", axum::routing::put(update_party).delete(delete_party))
        .route("/places", get(list_places).post(create_place))
        .route("/places/{id}", axum::routing::put(update_place).delete(delete_place))
        .route("/goods", get(list_goods).post(create_goods))
        .route("/goods/{id}", axum::routing::delete(delete_goods))
}

#[derive(Deserialize)]
struct Search {
    /// Texto a buscar (nombre, matrícula, NIF, dirección...)
    q: Option<String>,
    /// Solo para `/parties`: `cargador` o `transportista`
    tipo: Option<String>,
}

fn like(q: &Option<String>) -> String {
    format!("%{}%", q.as_deref().unwrap_or("").trim())
}

/// Comprueba que el usuario puede editar/borrar un elemento de la tabla.
async fn check_owner(state: &AppState, user: &CurrentUser, table: &str, id: i64) -> AppResult<()> {
    let row: Option<(Option<i64>,)> = sqlx::query_as(&format!("SELECT created_by FROM {table} WHERE id = ?"))
        .bind(id)
        .fetch_optional(&state.pool)
        .await?;
    let (owner,) = row.ok_or_else(|| AppError::not_found("No encontrado"))?;
    if user.is_admin() || owner == Some(user.id) {
        Ok(())
    } else {
        Err(AppError::bad("Solo quien lo añadió o el administrador puede cambiarlo"))
    }
}

async fn delete_from(state: &AppState, user: &CurrentUser, table: &str, id: i64) -> AppResult<Json<Value>> {
    check_owner(state, user, table, id).await?;
    sqlx::query(&format!("DELETE FROM {table} WHERE id = ?")).bind(id).execute(&state.pool).await?;
    Ok(Json(json!({ "ok": true })))
}

fn non_empty(v: &str, msg: &str) -> AppResult<String> {
    let v = v.trim();
    if v.is_empty() { Err(AppError::bad(msg)) } else { Ok(v.to_string()) }
}

// ===================== Remolques =====================

#[derive(Serialize, FromRow)]
struct Trailer {
    id: i64,
    nombre: String,
    matricula: String,
    created_at: DateTime<Utc>,
}

#[derive(Deserialize)]
struct TrailerReq {
    nombre: String,
    matricula: String,
}

async fn list_trailers(State(s): State<AppState>, _u: CurrentUser, Query(q): Query<Search>) -> AppResult<Json<Vec<Trailer>>> {
    Ok(Json(
        sqlx::query_as("SELECT id, nombre, matricula, created_at FROM trailers WHERE nombre LIKE ?1 OR matricula LIKE ?1 ORDER BY nombre")
            .bind(like(&q.q))
            .fetch_all(&s.pool)
            .await?,
    ))
}

async fn create_trailer(State(s): State<AppState>, u: CurrentUser, Json(r): Json<TrailerReq>) -> AppResult<Json<Trailer>> {
    let plate = non_empty(&normalize_plate(&r.matricula), "Escribe la matrícula del remolque")?;
    let name = if r.nombre.trim().is_empty() { plate.clone() } else { r.nombre.trim().to_string() };
    let row = sqlx::query_as(
        "INSERT INTO trailers (nombre, matricula, created_by, created_at) VALUES (?, ?, ?, ?)
         RETURNING id, nombre, matricula, created_at",
    )
    .bind(name).bind(plate).bind(u.id).bind(Utc::now())
    .fetch_one(&s.pool)
    .await
    .map_err(dup("Ya existe un remolque con esa matrícula"))?;
    Ok(Json(row))
}

async fn update_trailer(State(s): State<AppState>, u: CurrentUser, Path(id): Path<i64>, Json(r): Json<TrailerReq>) -> AppResult<Json<Value>> {
    check_owner(&s, &u, "trailers", id).await?;
    let plate = non_empty(&normalize_plate(&r.matricula), "Escribe la matrícula del remolque")?;
    sqlx::query("UPDATE trailers SET nombre = ?, matricula = ? WHERE id = ?")
        .bind(r.nombre.trim()).bind(plate).bind(id)
        .execute(&s.pool)
        .await
        .map_err(dup("Ya existe un remolque con esa matrícula"))?;
    Ok(Json(json!({ "ok": true })))
}

async fn delete_trailer(State(s): State<AppState>, u: CurrentUser, Path(id): Path<i64>) -> AppResult<Json<Value>> {
    delete_from(&s, &u, "trailers", id).await
}

fn dup(msg: &'static str) -> impl Fn(sqlx::Error) -> AppError {
    move |e| match AppError::from(e) {
        AppError::Conflict(_) => AppError::Conflict(msg.into()),
        other => other,
    }
}

// ===================== Camiones =====================

#[derive(Serialize, FromRow)]
struct Truck {
    id: i64,
    nombre: String,
    matricula: String,
    /// Remolque predeterminado: al elegir el camión se rellena solo.
    remolque_id: Option<i64>,
    remolque_nombre: Option<String>,
    remolque_matricula: Option<String>,
    created_at: DateTime<Utc>,
}

const TRUCK_SELECT: &str = "SELECT t.id, t.nombre, t.matricula, t.default_trailer_id AS remolque_id,
    r.nombre AS remolque_nombre, r.matricula AS remolque_matricula, t.created_at
    FROM trucks t LEFT JOIN trailers r ON r.id = t.default_trailer_id";

#[derive(Deserialize)]
struct TruckReq {
    nombre: String,
    matricula: String,
    /// Id de un remolque existente…
    remolque_id: Option<i64>,
    /// …o su matrícula (si no existe se crea).
    remolque_matricula: Option<String>,
}

async fn resolve_trailer(s: &AppState, u: &CurrentUser, r: &TruckReq) -> AppResult<Option<i64>> {
    if let Some(id) = r.remolque_id {
        return Ok(Some(id));
    }
    match r.remolque_matricula.as_deref().map(normalize_plate).filter(|p| !p.is_empty()) {
        Some(plate) => Ok(Some(ensure_trailer(s, u.id, &plate).await?)),
        None => Ok(None),
    }
}

async fn ensure_trailer(s: &AppState, user_id: i64, plate: &str) -> AppResult<i64> {
    sqlx::query("INSERT OR IGNORE INTO trailers (nombre, matricula, created_by, created_at) VALUES (?, ?, ?, ?)")
        .bind(plate).bind(plate).bind(user_id).bind(Utc::now())
        .execute(&s.pool)
        .await?;
    Ok(sqlx::query_scalar("SELECT id FROM trailers WHERE matricula = ?").bind(plate).fetch_one(&s.pool).await?)
}

async fn list_trucks(State(s): State<AppState>, _u: CurrentUser, Query(q): Query<Search>) -> AppResult<Json<Vec<Truck>>> {
    Ok(Json(
        sqlx::query_as(&format!("{TRUCK_SELECT} WHERE t.nombre LIKE ?1 OR t.matricula LIKE ?1 ORDER BY t.nombre"))
            .bind(like(&q.q))
            .fetch_all(&s.pool)
            .await?,
    ))
}

async fn fetch_truck(s: &AppState, id: i64) -> AppResult<Truck> {
    sqlx::query_as(&format!("{TRUCK_SELECT} WHERE t.id = ?"))
        .bind(id)
        .fetch_optional(&s.pool)
        .await?
        .ok_or_else(|| AppError::not_found("Camión no encontrado"))
}

async fn get_truck(State(s): State<AppState>, _u: CurrentUser, Path(id): Path<i64>) -> AppResult<Json<Truck>> {
    Ok(Json(fetch_truck(&s, id).await?))
}

async fn create_truck(State(s): State<AppState>, u: CurrentUser, Json(r): Json<TruckReq>) -> AppResult<Json<Truck>> {
    let plate = non_empty(&normalize_plate(&r.matricula), "Escribe la matrícula del camión")?;
    let name = if r.nombre.trim().is_empty() { plate.clone() } else { r.nombre.trim().to_string() };
    let trailer = resolve_trailer(&s, &u, &r).await?;
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO trucks (nombre, matricula, default_trailer_id, created_by, created_at) VALUES (?, ?, ?, ?, ?) RETURNING id",
    )
    .bind(name).bind(plate).bind(trailer).bind(u.id).bind(Utc::now())
    .fetch_one(&s.pool)
    .await
    .map_err(dup("Ya existe un camión con esa matrícula"))?;
    Ok(Json(fetch_truck(&s, id).await?))
}

async fn update_truck(State(s): State<AppState>, u: CurrentUser, Path(id): Path<i64>, Json(r): Json<TruckReq>) -> AppResult<Json<Truck>> {
    check_owner(&s, &u, "trucks", id).await?;
    let plate = non_empty(&normalize_plate(&r.matricula), "Escribe la matrícula del camión")?;
    let trailer = resolve_trailer(&s, &u, &r).await?;
    sqlx::query("UPDATE trucks SET nombre = ?, matricula = ?, default_trailer_id = ? WHERE id = ?")
        .bind(r.nombre.trim()).bind(plate).bind(trailer).bind(id)
        .execute(&s.pool)
        .await
        .map_err(dup("Ya existe un camión con esa matrícula"))?;
    Ok(Json(fetch_truck(&s, id).await?))
}

async fn delete_truck(State(s): State<AppState>, u: CurrentUser, Path(id): Path<i64>) -> AppResult<Json<Value>> {
    delete_from(&s, &u, "trucks", id).await
}

// ===================== Empresas cargadoras / transportistas =====================

#[derive(Serialize, FromRow)]
struct Party {
    id: i64,
    /// `cargador` (empresa que carga el camión) o `transportista` (empresa que transporta)
    tipo: String,
    nombre: String,
    nif: String,
    domicilio: String,
    /// Solo transportistas: "Nombre transportista", dueño de los vehículos
    titular: String,
    created_at: DateTime<Utc>,
    /// Ubicaciones de la empresa: al elegirla en un DeCA se proponen como dirección.
    #[sqlx(skip)]
    ubicaciones: Vec<Ubicacion>,
}

#[derive(Clone, Serialize, FromRow)]
struct Ubicacion {
    id: i64,
    nombre: String,
    direccion: String,
}

#[derive(Deserialize)]
struct PartyReq {
    tipo: String,
    nombre: String,
    nif: String,
    #[serde(default)]
    domicilio: String,
    #[serde(default)]
    titular: String,
    /// Direcciones completas de la empresa. Si se omite, no se cambian.
    ubicaciones: Option<Vec<String>>,
}

const MAX_UBICACIONES: usize = 50;

/// Añade a cada empresa sus ubicaciones.
async fn with_places(s: &AppState, mut parties: Vec<Party>) -> AppResult<Vec<Party>> {
    let rows: Vec<(i64, i64, String, String)> = sqlx::query_as(
        "SELECT pp.party_id, p.id, p.nombre, p.direccion
         FROM party_places pp JOIN places p ON p.id = pp.place_id ORDER BY p.nombre",
    )
    .fetch_all(&s.pool)
    .await?;
    for party in &mut parties {
        party.ubicaciones = rows
            .iter()
            .filter(|(owner, ..)| *owner == party.id)
            .map(|(_, id, nombre, direccion)| Ubicacion { id: *id, nombre: nombre.clone(), direccion: direccion.clone() })
            .collect();
    }
    Ok(parties)
}

/// Sustituye las ubicaciones de una empresa; las direcciones nuevas se guardan también
/// en la base de datos compartida.
async fn set_places(s: &AppState, user_id: i64, party_id: i64, addresses: &[String]) -> AppResult<()> {
    if addresses.len() > MAX_UBICACIONES {
        return Err(AppError::bad(format!("Máximo {MAX_UBICACIONES} ubicaciones por empresa")));
    }
    let mut tx = s.pool.begin().await?;
    sqlx::query("DELETE FROM party_places WHERE party_id = ?").bind(party_id).execute(&mut *tx).await?;
    for address in addresses.iter().map(|a| a.trim()).filter(|a| !a.is_empty()) {
        sqlx::query("INSERT OR IGNORE INTO places (nombre, direccion, created_by, created_at) VALUES (?, ?, ?, ?)")
            .bind(address).bind(address).bind(user_id).bind(Utc::now())
            .execute(&mut *tx)
            .await?;
        sqlx::query("INSERT OR IGNORE INTO party_places (party_id, place_id) SELECT ?, id FROM places WHERE direccion = ?")
            .bind(party_id).bind(address)
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await?;
    Ok(())
}

fn check_tipo(t: &str) -> AppResult<()> {
    if t == "cargador" || t == "transportista" {
        Ok(())
    } else {
        Err(AppError::bad("El tipo debe ser «cargador» o «transportista»"))
    }
}

async fn list_parties(State(s): State<AppState>, _u: CurrentUser, Query(q): Query<Search>) -> AppResult<Json<Vec<Party>>> {
    let parties: Vec<Party> = sqlx::query_as(
        "SELECT id, tipo, nombre, nif, domicilio, titular, created_at FROM parties
         WHERE (?2 IS NULL OR tipo = ?2) AND (nombre LIKE ?1 OR nif LIKE ?1) ORDER BY nombre",
    )
    .bind(like(&q.q))
    .bind(q.tipo)
    .fetch_all(&s.pool)
    .await?;
    Ok(Json(with_places(&s, parties).await?))
}

async fn create_party(State(s): State<AppState>, u: CurrentUser, Json(r): Json<PartyReq>) -> AppResult<Json<Party>> {
    check_tipo(&r.tipo)?;
    let row: Party = sqlx::query_as(
        "INSERT INTO parties (tipo, nombre, nif, domicilio, titular, created_by, created_at) VALUES (?, ?, ?, ?, ?, ?, ?)
         RETURNING id, tipo, nombre, nif, domicilio, titular, created_at",
    )
    .bind(&r.tipo)
    .bind(non_empty(&r.nombre, "Escribe el nombre de la empresa")?)
    .bind(non_empty(&r.nif.to_uppercase(), "Escribe el NIF/CIF")?)
    .bind(r.domicilio.trim())
    .bind(r.titular.trim())
    .bind(u.id)
    .bind(Utc::now())
    .fetch_one(&s.pool)
    .await
    .map_err(dup("Ya existe una empresa con ese NIF/CIF"))?;
    if let Some(list) = &r.ubicaciones {
        set_places(&s, u.id, row.id, list).await?;
    }
    let mut out = with_places(&s, vec![row]).await?;
    Ok(Json(out.remove(0)))
}

async fn update_party(State(s): State<AppState>, u: CurrentUser, Path(id): Path<i64>, Json(r): Json<PartyReq>) -> AppResult<Json<Value>> {
    check_owner(&s, &u, "parties", id).await?;
    check_tipo(&r.tipo)?;
    sqlx::query("UPDATE parties SET tipo = ?, nombre = ?, nif = ?, domicilio = ?, titular = ? WHERE id = ?")
        .bind(&r.tipo)
        .bind(non_empty(&r.nombre, "Escribe el nombre de la empresa")?)
        .bind(non_empty(&r.nif.to_uppercase(), "Escribe el NIF/CIF")?)
        .bind(r.domicilio.trim())
        .bind(r.titular.trim())
        .bind(id)
        .execute(&s.pool)
        .await
        .map_err(dup("Ya existe una empresa con ese NIF/CIF"))?;
    if let Some(list) = &r.ubicaciones {
        set_places(&s, u.id, id, list).await?;
    }
    Ok(Json(json!({ "ok": true })))
}

async fn delete_party(State(s): State<AppState>, u: CurrentUser, Path(id): Path<i64>) -> AppResult<Json<Value>> {
    delete_from(&s, &u, "parties", id).await
}

// ===================== Direcciones (origen / destino) =====================

#[derive(Serialize, FromRow)]
struct Place {
    id: i64,
    nombre: String,
    direccion: String,
    lat: Option<f64>,
    lon: Option<f64>,
    created_at: DateTime<Utc>,
}

#[derive(Deserialize)]
struct PlaceReq {
    #[serde(default)]
    nombre: String,
    direccion: String,
    lat: Option<f64>,
    lon: Option<f64>,
}

async fn list_places(State(s): State<AppState>, _u: CurrentUser, Query(q): Query<Search>) -> AppResult<Json<Vec<Place>>> {
    Ok(Json(
        sqlx::query_as("SELECT id, nombre, direccion, lat, lon, created_at FROM places WHERE nombre LIKE ?1 OR direccion LIKE ?1 ORDER BY nombre")
            .bind(like(&q.q))
            .fetch_all(&s.pool)
            .await?,
    ))
}

async fn create_place(State(s): State<AppState>, u: CurrentUser, Json(r): Json<PlaceReq>) -> AppResult<Json<Place>> {
    let dir = non_empty(&r.direccion, "Escribe la dirección completa")?;
    let name = if r.nombre.trim().is_empty() { dir.clone() } else { r.nombre.trim().to_string() };
    let row = sqlx::query_as(
        "INSERT INTO places (nombre, direccion, lat, lon, created_by, created_at) VALUES (?, ?, ?, ?, ?, ?)
         RETURNING id, nombre, direccion, lat, lon, created_at",
    )
    .bind(name).bind(dir).bind(r.lat).bind(r.lon).bind(u.id).bind(Utc::now())
    .fetch_one(&s.pool)
    .await
    .map_err(dup("Esa dirección ya está guardada"))?;
    Ok(Json(row))
}

async fn update_place(State(s): State<AppState>, u: CurrentUser, Path(id): Path<i64>, Json(r): Json<PlaceReq>) -> AppResult<Json<Value>> {
    check_owner(&s, &u, "places", id).await?;
    let dir = non_empty(&r.direccion, "Escribe la dirección completa")?;
    sqlx::query("UPDATE places SET nombre = ?, direccion = ?, lat = ?, lon = ? WHERE id = ?")
        .bind(r.nombre.trim()).bind(dir).bind(r.lat).bind(r.lon).bind(id)
        .execute(&s.pool)
        .await
        .map_err(dup("Esa dirección ya está guardada"))?;
    Ok(Json(json!({ "ok": true })))
}

async fn delete_place(State(s): State<AppState>, u: CurrentUser, Path(id): Path<i64>) -> AppResult<Json<Value>> {
    delete_from(&s, &u, "places", id).await
}

// ===================== Mercancías =====================

#[derive(Serialize, FromRow)]
struct Goods {
    id: i64,
    descripcion: String,
    created_at: DateTime<Utc>,
}

#[derive(Deserialize)]
struct GoodsReq {
    descripcion: String,
}

async fn list_goods(State(s): State<AppState>, _u: CurrentUser, Query(q): Query<Search>) -> AppResult<Json<Vec<Goods>>> {
    Ok(Json(
        sqlx::query_as("SELECT id, descripcion, created_at FROM goods WHERE descripcion LIKE ? ORDER BY descripcion")
            .bind(like(&q.q))
            .fetch_all(&s.pool)
            .await?,
    ))
}

async fn create_goods(State(s): State<AppState>, u: CurrentUser, Json(r): Json<GoodsReq>) -> AppResult<Json<Goods>> {
    let row = sqlx::query_as("INSERT INTO goods (descripcion, created_by, created_at) VALUES (?, ?, ?) RETURNING id, descripcion, created_at")
        .bind(non_empty(&r.descripcion, "Escribe la mercancía")?)
        .bind(u.id)
        .bind(Utc::now())
        .fetch_one(&s.pool)
        .await
        .map_err(dup("Esa mercancía ya está guardada"))?;
    Ok(Json(row))
}

async fn delete_goods(State(s): State<AppState>, u: CurrentUser, Path(id): Path<i64>) -> AppResult<Json<Value>> {
    delete_from(&s, &u, "goods", id).await
}

// ===================== Aprendizaje automático desde un DeCA =====================

/// Guarda en la base de datos compartida los datos nuevos de un DeCA emitido
/// (empresas, direcciones, mercancías, camión y remolque) para poder reutilizarlos.
pub async fn remember(s: &AppState, user_id: i64, d: &DecaDatos) -> AppResult<()> {
    let now = Utc::now();
    let t = &d.transportista;
    for (tipo, nombre, nif, domicilio, titular) in [
        ("cargador", &d.cargador.nombre, &d.cargador.nif, &d.cargador.domicilio, ""),
        ("transportista", &t.nombre, &t.nif, &t.domicilio, t.titular.as_deref().unwrap_or("")),
    ] {
        sqlx::query("INSERT OR IGNORE INTO parties (tipo, nombre, nif, domicilio, titular, created_by, created_at) VALUES (?, ?, ?, ?, ?, ?, ?)")
            .bind(tipo).bind(nombre).bind(nif).bind(domicilio).bind(titular).bind(user_id).bind(now)
            .execute(&s.pool)
            .await?;
    }
    for e in &d.envios {
        for dir in [&e.origen, &e.destino] {
            sqlx::query("INSERT OR IGNORE INTO places (nombre, direccion, created_by, created_at) VALUES (?, ?, ?, ?)")
                .bind(dir).bind(dir).bind(user_id).bind(now)
                .execute(&s.pool)
                .await?;
        }
        sqlx::query("INSERT OR IGNORE INTO goods (descripcion, created_by, created_at) VALUES (?, ?, ?)")
            .bind(&e.mercancia).bind(user_id).bind(now)
            .execute(&s.pool)
            .await?;
    }
    // Como el remolque de un camión nuevo: si la empresa cargadora aún no tiene
    // ubicaciones, la primera vez se le asocia el origen del DeCA.
    if let Some(first) = d.envios.first() {
        sqlx::query(
            "INSERT OR IGNORE INTO party_places (party_id, place_id)
             SELECT py.id, pl.id FROM parties py, places pl
             WHERE py.tipo = 'cargador' AND py.nif = ?1 AND pl.direccion = ?2
               AND NOT EXISTS (SELECT 1 FROM party_places x WHERE x.party_id = py.id)",
        )
        .bind(&d.cargador.nif)
        .bind(&first.origen)
        .execute(&s.pool)
        .await?;
    }
    let trailer_id = match &d.matricula_remolque {
        Some(plate) => Some(ensure_trailer(s, user_id, plate).await?),
        None => None,
    };
    sqlx::query("INSERT OR IGNORE INTO trucks (nombre, matricula, default_trailer_id, created_by, created_at) VALUES (?, ?, ?, ?, ?)")
        .bind(&d.matricula_camion).bind(&d.matricula_camion).bind(trailer_id).bind(user_id).bind(now)
        .execute(&s.pool)
        .await?;
    Ok(())
}
