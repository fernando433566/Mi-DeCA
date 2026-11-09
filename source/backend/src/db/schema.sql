-- Esquema de la base de datos del servidor DeCA (SQLite).
-- Fechas en texto RFC 3339 (UTC). Preferencias y datos de DeCA en JSON.

CREATE TABLE IF NOT EXISTS companies (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    name        TEXT NOT NULL,
    cif         TEXT NOT NULL DEFAULT '',
    domicilio   TEXT NOT NULL DEFAULT '',
    telefono    TEXT NOT NULL DEFAULT '',
    -- "Nombre transportista": persona o sociedad dueña de los vehículos
    titular     TEXT NOT NULL DEFAULT '',
    -- Preferencias fijadas por el administrador para toda la empresa
    preferences TEXT NOT NULL DEFAULT '{}',
    created_at  TEXT NOT NULL,
    updated_at  TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS users (
    id                  INTEGER PRIMARY KEY AUTOINCREMENT,
    username            TEXT NOT NULL UNIQUE COLLATE NOCASE,
    display_name        TEXT NOT NULL,
    password_hash       TEXT NOT NULL,
    role                TEXT NOT NULL CHECK (role IN ('admin', 'worker')),
    company_id          INTEGER REFERENCES companies(id) ON DELETE SET NULL,
    totp_secret         TEXT,
    totp_enabled        INTEGER NOT NULL DEFAULT 0,
    totp_pending_secret TEXT,
    -- Preferencias que el administrador fija para este trabajador (bloqueadas)
    prefs_admin         TEXT NOT NULL DEFAULT '{}',
    -- Preferencias que el propio trabajador fija
    prefs_own           TEXT NOT NULL DEFAULT '{}',
    active              INTEGER NOT NULL DEFAULT 1,
    created_at          TEXT NOT NULL,
    updated_at          TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS sessions (
    token_hash   TEXT PRIMARY KEY,
    user_id      INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    created_at   TEXT NOT NULL,
    expires_at   TEXT NOT NULL,
    last_used_at TEXT NOT NULL
);

-- Códigos QR de acceso automático (un solo uso, caducan en minutos)
CREATE TABLE IF NOT EXISTS qr_logins (
    token_hash TEXT PRIMARY KEY,
    user_id    INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    created_at TEXT NOT NULL,
    expires_at TEXT NOT NULL,
    used       INTEGER NOT NULL DEFAULT 0
);

-- ===== Base de datos compartida (todos los usuarios pueden añadir y usar) =====

CREATE TABLE IF NOT EXISTS trailers (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    nombre     TEXT NOT NULL,
    matricula  TEXT NOT NULL UNIQUE COLLATE NOCASE,
    created_by INTEGER REFERENCES users(id) ON DELETE SET NULL,
    created_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS trucks (
    id                 INTEGER PRIMARY KEY AUTOINCREMENT,
    nombre             TEXT NOT NULL,
    matricula          TEXT NOT NULL UNIQUE COLLATE NOCASE,
    default_trailer_id INTEGER REFERENCES trailers(id) ON DELETE SET NULL,
    created_by         INTEGER REFERENCES users(id) ON DELETE SET NULL,
    created_at         TEXT NOT NULL
);

-- Empresas cargadoras y transportistas (nombre, NIF/CIF y domicilio)
CREATE TABLE IF NOT EXISTS parties (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    tipo       TEXT NOT NULL CHECK (tipo IN ('cargador', 'transportista')),
    nombre     TEXT NOT NULL,
    nif        TEXT NOT NULL COLLATE NOCASE,
    domicilio  TEXT NOT NULL DEFAULT '',
    -- Solo transportistas: dueño de los vehículos
    titular    TEXT NOT NULL DEFAULT '',
    created_by INTEGER REFERENCES users(id) ON DELETE SET NULL,
    created_at TEXT NOT NULL,
    UNIQUE (tipo, nif)
);

-- Direcciones de origen y destino (OpenStreetMap)
CREATE TABLE IF NOT EXISTS places (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    nombre     TEXT NOT NULL,
    direccion  TEXT NOT NULL UNIQUE COLLATE NOCASE,
    lat        REAL,
    lon        REAL,
    created_by INTEGER REFERENCES users(id) ON DELETE SET NULL,
    created_at TEXT NOT NULL
);

-- Ubicaciones de cada empresa (cargadora o transportista)
CREATE TABLE IF NOT EXISTS party_places (
    party_id INTEGER NOT NULL REFERENCES parties(id) ON DELETE CASCADE,
    place_id INTEGER NOT NULL REFERENCES places(id) ON DELETE CASCADE,
    PRIMARY KEY (party_id, place_id)
);

-- Tipos de mercancía
CREATE TABLE IF NOT EXISTS goods (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    descripcion TEXT NOT NULL UNIQUE COLLATE NOCASE,
    created_by  INTEGER REFERENCES users(id) ON DELETE SET NULL,
    created_at  TEXT NOT NULL
);

-- ===== DeCA =====

CREATE TABLE IF NOT EXISTS decas (
    id               INTEGER PRIMARY KEY AUTOINCREMENT,
    numero           TEXT NOT NULL DEFAULT '',
    company_id       INTEGER REFERENCES companies(id) ON DELETE SET NULL,
    user_id          INTEGER NOT NULL REFERENCES users(id),
    user_name        TEXT NOT NULL,
    token            TEXT NOT NULL UNIQUE,
    file_rel_path    TEXT NOT NULL,
    file_name        TEXT NOT NULL,
    url_path         TEXT NOT NULL,
    public_url       TEXT NOT NULL,
    datos            TEXT NOT NULL,
    version          INTEGER NOT NULL DEFAULT 1,
    modificaciones   TEXT NOT NULL DEFAULT '[]',
    matricula_camion TEXT NOT NULL DEFAULT '',
    fecha_transporte TEXT NOT NULL,
    viaje_num        INTEGER NOT NULL DEFAULT 1,
    viaje_total      INTEGER NOT NULL DEFAULT 1,
    grupo_id         TEXT NOT NULL,
    created_at       TEXT NOT NULL,
    updated_at       TEXT NOT NULL,
    expires_at       TEXT NOT NULL,
    pdf_deleted      INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX IF NOT EXISTS idx_decas_created ON decas (created_at);
CREATE INDEX IF NOT EXISTS idx_decas_user ON decas (user_id, created_at);

-- Histórico de versiones (cada modificación conserva el PDF anterior)
CREATE TABLE IF NOT EXISTS deca_versions (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    deca_id       INTEGER NOT NULL REFERENCES decas(id) ON DELETE CASCADE,
    version       INTEGER NOT NULL,
    file_rel_path TEXT NOT NULL,
    datos         TEXT NOT NULL,
    created_at    TEXT NOT NULL
);

-- Registro de fecha y hora de creaciones, modificaciones y descargas
CREATE TABLE IF NOT EXISTS audit_log (
    id      INTEGER PRIMARY KEY AUTOINCREMENT,
    ts      TEXT NOT NULL,
    user_id INTEGER,
    action  TEXT NOT NULL,
    deca_id INTEGER,
    detail  TEXT NOT NULL DEFAULT ''
);
CREATE INDEX IF NOT EXISTS idx_audit_deca ON audit_log (deca_id);

-- Ajustes del servidor (clave/valor JSON)
CREATE TABLE IF NOT EXISTS settings (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
);

-- Mapas por regiones para usar sin conexión (ver src/maps/regions.rs)
CREATE TABLE IF NOT EXISTS map_packages (
    region_id  TEXT PRIMARY KEY,
    estado     TEXT NOT NULL,
    progreso   REAL NOT NULL DEFAULT 0,
    version    TEXT,
    max_zoom   INTEGER,
    size       INTEGER,
    sha256     TEXT,
    file_rel   TEXT,
    error      TEXT,
    updated_at TEXT NOT NULL
);
