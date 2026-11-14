//! Lógica de negocio de los DeCA: emisión atómica (PDF + URL + QR + registro),
//! modificaciones con histórico de versiones y consultas.

pub mod layout;
pub mod model;
pub mod pdf;
pub mod qr;
pub mod storage;

use chrono::{DateTime, Duration, NaiveTime, TimeZone, Utc};
use chrono_tz::Europe::Madrid;
use serde::Serialize;
use sqlx::FromRow;

use crate::auth::{random_token, CurrentUser};
use crate::db::{self, settings};
use crate::error::{AppError, AppResult};
use crate::state::AppState;
use model::{DecaDatos, Modificacion};

pub const MAX_VIAJES: u32 = 30;

/// Fila de la tabla `decas`.
#[derive(Clone, Debug, FromRow)]
pub struct DecaRow {
    pub id: i64,
    pub numero: String,
    pub company_id: Option<i64>,
    pub user_id: i64,
    pub user_name: String,
    pub file_rel_path: String,
    pub file_name: String,
    pub url_path: String,
    pub public_url: String,
    pub datos: String,
    pub version: i64,
    pub modificaciones: String,
    pub viaje_num: i64,
    pub viaje_total: i64,
    pub grupo_id: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub pdf_deleted: bool,
}

/// Lo que la app recibe de un DeCA (pantalla de detalle, ticket y widget).
#[derive(Debug, Serialize)]
pub struct DecaView {
    pub id: i64,
    pub numero: String,
    pub empresa_id: Option<i64>,
    pub emitido_por: String,
    pub emitido_por_id: i64,
    /// URL única del QR (descarga directa del PDF, sin inicio de sesión).
    pub url: String,
    /// `false` cuando ya pasaron los días de enlace público.
    pub url_activa: bool,
    pub url_caduca: DateTime<Utc>,
    /// El PDF original sigue guardado (conservación de 1 año).
    pub pdf_disponible: bool,
    pub viaje_num: i64,
    pub viaje_total: i64,
    pub grupo_id: String,
    pub version: i64,
    pub creado: DateTime<Utc>,
    pub modificado: DateTime<Utc>,
    pub datos: DecaDatos,
    pub modificaciones: Vec<Modificacion>,
    /// Rutas de la API (requieren sesión) para el QR y el PDF.
    pub qr_svg: String,
    pub qr_png: String,
    pub pdf: String,
}

impl DecaRow {
    pub fn datos(&self) -> AppResult<DecaDatos> {
        Ok(serde_json::from_str(&self.datos)?)
    }

    pub fn mods(&self) -> AppResult<Vec<Modificacion>> {
        Ok(serde_json::from_str(&self.modificaciones)?)
    }

    pub fn view(&self) -> AppResult<DecaView> {
        Ok(DecaView {
            id: self.id,
            numero: self.numero.clone(),
            empresa_id: self.company_id,
            emitido_por: self.user_name.clone(),
            emitido_por_id: self.user_id,
            url: self.public_url.clone(),
            url_activa: !self.pdf_deleted && self.expires_at > Utc::now(),
            url_caduca: self.expires_at,
            pdf_disponible: !self.pdf_deleted,
            viaje_num: self.viaje_num,
            viaje_total: self.viaje_total,
            grupo_id: self.grupo_id.clone(),
            version: self.version,
            creado: self.created_at,
            modificado: self.updated_at,
            datos: self.datos()?,
            modificaciones: self.mods()?,
            qr_svg: format!("/api/decas/{}/qr.svg", self.id),
            qr_png: format!("/api/decas/{}/qr.png", self.id),
            pdf: format!("/api/decas/{}/pdf", self.id),
        })
    }

    /// Solo el emisor o el administrador pueden ver o modificar un DeCA.
    pub fn check_access(&self, user: &CurrentUser) -> AppResult<()> {
        if user.is_admin() || user.id == self.user_id {
            Ok(())
        } else {
            Err(AppError::Forbidden)
        }
    }
}

pub async fn get_row(state: &AppState, id: i64) -> AppResult<DecaRow> {
    sqlx::query_as("SELECT * FROM decas WHERE id = ?")
        .bind(id)
        .fetch_optional(&state.pool)
        .await?
        .ok_or_else(|| AppError::not_found("DeCA no encontrado"))
}

/// El enlace público dura al menos `days` días naturales tras el fin del servicio
/// (y nunca menos de `days` días desde la emisión).
fn expiry(datos: &DecaDatos, created: DateTime<Utc>, days: i64) -> DateTime<Utc> {
    let end_local = datos.service_end().and_time(NaiveTime::from_hms_opt(23, 59, 59).unwrap());
    let end = Madrid
        .from_local_datetime(&end_local)
        .latest()
        .map(|d| d.with_timezone(&Utc))
        .unwrap_or(created);
    (end + Duration::days(days)).max(created + Duration::days(days))
}

/// Emite uno o varios DeCA (uno por viaje). Cada uno se genera de forma atómica:
/// registro + PDF con QR en disco; si algo falla no queda nada a medias.
pub async fn create(
    state: &AppState,
    user: &CurrentUser,
    company_id: Option<i64>,
    mut datos: DecaDatos,
    viajes: u32,
    base_url: &str,
    client_ref: Option<&str>,
) -> AppResult<Vec<DecaRow>> {
    datos.normalize_and_validate()?;
    if viajes == 0 || viajes > MAX_VIAJES {
        return Err(AppError::bad(format!("El número de viajes debe estar entre 1 y {MAX_VIAJES}")));
    }
    let base = base_url.trim_end_matches('/');
    let cfg = settings::load(&state.pool).await?;
    let grupo_id = client_ref.map(str::to_string).unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
    let deca_dir = state.config.deca_dir();
    let mut created = Vec::new();
    for viaje in 1..=viajes as i64 {
        // Viajes de ida y vuelta: 1 = A -> B, 2 = B -> A, 3 = A -> B…
        let trip = datos.for_trip(viaje);
        let datos_json = serde_json::to_string(&trip)?;
        let now = Utc::now();
        let token = random_token();
        let reserved = storage::reserve_location(&deca_dir, &user.display_name, &datos.matricula_camion, now)?;
        let loc = &reserved.location;
        let url_path = loc.url_path(&token);
        let public_url = format!("{base}{url_path}");
        let expires = expiry(&datos, now, cfg.public_link_days);

        let mut tx = state.pool.begin().await?;
        let id: i64 = sqlx::query_scalar(
            "INSERT INTO decas (company_id, user_id, user_name, token, file_rel_path, file_name, url_path,
                public_url, datos, matricula_camion, fecha_transporte, viaje_num, viaje_total, grupo_id,
                created_at, updated_at, expires_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?) RETURNING id",
        )
        .bind(company_id)
        .bind(user.id)
        .bind(&user.display_name)
        .bind(&token)
        .bind(loc.rel_path())
        .bind(&loc.file_name)
        .bind(&url_path)
        .bind(&public_url)
        .bind(&datos_json)
        .bind(&trip.matricula_camion)
        .bind(trip.fecha_transporte.to_string())
        .bind(viaje)
        .bind(viajes as i64)
        .bind(&grupo_id)
        .bind(now)
        .bind(now)
        .bind(expires)
        .fetch_one(&mut *tx)
        .await?;
        let numero = format!("DECA-{}-{id:06}", now.with_timezone(&Madrid).format("%Y"));
        sqlx::query("UPDATE decas SET numero = ? WHERE id = ?")
            .bind(&numero)
            .bind(id)
            .execute(&mut *tx)
            .await?;

        let bytes = pdf::render(&pdf::RenderInput {
            numero: &numero,
            url: &public_url,
            emitido_por: &user.display_name,
            viaje_num: viaje,
            viaje_total: viajes as i64,
            version: 1,
            created: now,
            modified: now,
            datos: &trip,
            modificaciones: &[],
        })?;
        let path = storage::absolute(&deca_dir, &loc.rel_path());
        storage::write_atomic(&path, &bytes).await?;
        verify_file(&path).await?;

        // Si falla, al salir se borra el fichero reservado (ver `storage::Reserved`).
        tx.commit().await?;
        let rel_path = loc.rel_path();
        reserved.keep();
        db::audit(&state.pool, Some(user.id), "deca_creado", Some(id), format!("{numero} · {rel_path}")).await;
        created.push(get_row(state, id).await?);
    }
    Ok(created)
}

/// Comprobación tras escribir: el fichero existe, es un PDF y no supera 5 MB.
async fn verify_file(path: &std::path::Path) -> AppResult<()> {
    let bytes = tokio::fs::read(path).await?;
    if !bytes.starts_with(b"%PDF-") || bytes.len() > pdf::MAX_PDF_BYTES {
        let _ = tokio::fs::remove_file(path).await;
        return Err(AppError::Internal(anyhow::anyhow!("el PDF generado no es válido")));
    }
    Ok(())
}

/// Modifica un DeCA durante el servicio (apartado Quinto, método 1): se regenera el
/// mismo PDF (misma URL y mismo QR) con los datos nuevos, los antiguos tachados como
/// NO VÁLIDOS y el motivo. La versión anterior se conserva como `...-vN.pdf`.
pub async fn modify(
    state: &AppState,
    user: &CurrentUser,
    id: i64,
    mut nuevos: DecaDatos,
    motivo: &str,
) -> AppResult<DecaRow> {
    let row = get_row(state, id).await?;
    row.check_access(user)?;
    if row.pdf_deleted {
        return Err(AppError::Gone("El PDF de este DeCA ya fue eliminado".into()));
    }
    let motivo = motivo.trim();
    if motivo.is_empty() {
        return Err(AppError::bad("Indica el motivo de la modificación"));
    }
    nuevos.normalize_and_validate()?;
    let antiguos = row.datos()?;
    let cambios = antiguos.diff(&nuevos);
    if cambios.is_empty() {
        return Err(AppError::bad("No has cambiado ningún dato"));
    }

    let now = Utc::now();
    let new_version = row.version + 1;
    let mut mods = row.mods()?;
    mods.push(Modificacion {
        fecha: now,
        usuario: user.display_name.clone(),
        motivo: motivo.to_string(),
        version: new_version,
        cambios,
    });

    let deca_dir = state.config.deca_dir();
    let path = storage::absolute(&deca_dir, &row.file_rel_path);
    let archived_rel = row.file_rel_path.replace(".pdf", &format!("-v{}.pdf", row.version));
    let archived = storage::absolute(&deca_dir, &archived_rel);
    tokio::fs::copy(&path, &archived).await?;

    let cfg = settings::load(&state.pool).await?;
    let expires = expiry(&nuevos, row.created_at, cfg.public_link_days).max(row.expires_at);
    let bytes = pdf::render(&pdf::RenderInput {
        numero: &row.numero,
        url: &row.public_url,
        emitido_por: &row.user_name,
        viaje_num: row.viaje_num,
        viaje_total: row.viaje_total,
        version: new_version,
        created: row.created_at,
        modified: now,
        datos: &nuevos,
        modificaciones: &mods,
    })?;

    let mut tx = state.pool.begin().await?;
    sqlx::query("INSERT INTO deca_versions (deca_id, version, file_rel_path, datos, created_at) VALUES (?, ?, ?, ?, ?)")
        .bind(id)
        .bind(row.version)
        .bind(&archived_rel)
        .bind(&row.datos)
        .bind(row.updated_at)
        .execute(&mut *tx)
        .await?;
    sqlx::query(
        "UPDATE decas SET datos = ?, version = ?, modificaciones = ?, matricula_camion = ?,
            fecha_transporte = ?, updated_at = ?, expires_at = ? WHERE id = ?",
    )
    .bind(serde_json::to_string(&nuevos)?)
    .bind(new_version)
    .bind(serde_json::to_string(&mods)?)
    .bind(&nuevos.matricula_camion)
    .bind(nuevos.fecha_transporte.to_string())
    .bind(now)
    .bind(expires)
    .bind(id)
    .execute(&mut *tx)
    .await?;
    let written = async {
        storage::write_atomic(&path, &bytes).await?;
        verify_file(&path).await?;
        tx.commit().await?;
        Ok::<_, AppError>(())
    }
    .await;
    if let Err(e) = written {
        // Deja el PDF como estaba: misma URL, misma versión vigente.
        let _ = tokio::fs::copy(&archived, &path).await;
        let _ = tokio::fs::remove_file(&archived).await;
        return Err(e);
    }

    db::audit(
        &state.pool,
        Some(user.id),
        "deca_modificado",
        Some(id),
        format!("{} · versión {new_version} · motivo: {motivo}", row.numero),
    )
    .await;
    get_row(state, id).await
}

#[derive(Debug, Serialize, FromRow)]
pub struct VersionInfo {
    pub version: i64,
    pub created_at: DateTime<Utc>,
}

pub async fn versions(state: &AppState, id: i64) -> AppResult<Vec<VersionInfo>> {
    Ok(sqlx::query_as("SELECT version, created_at FROM deca_versions WHERE deca_id = ? ORDER BY version")
        .bind(id)
        .fetch_all(&state.pool)
        .await?)
}
