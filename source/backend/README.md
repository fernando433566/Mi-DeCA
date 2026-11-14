# DeCA · Backend (servidor + aplicación web)

Servidor en Rust que genera los **DeCA** (Documento electrónico de Control Administrativo) según la
Orden FOM/2861/2012 (art. 6) y la Resolución de la DGTCF de 5 de junio de 2026 (BOE-A-2026-12784),
y que sirve en el **puerto 8134** la aplicación web (carpeta `web/`). Todo va en una imagen Docker x86_64.

- Base de datos SQLite + PDF en disco, todo dentro de `/data`.
- En el primer arranque se crea la cuenta **`Admin` / `1234`** (o con la contraseña de `DECA_ADMIN_PASSWORD` / `DECA_ADMIN_PASSWORD_FILE`, 12+ caracteres). Mientras conserve `1234`, el servidor solo le deja cambiarla (`must_change_password`, `src/auth/mod.rs`).
- Cloudflare Tunnel va aparte y apunta a `http://<host>:8134`.

La app Android está en [`../frontend`](../frontend) y usa esta misma web.

## Compilar y ejecutar

```bash
# Desarrollo (Rust 1.99)
cargo run                     # http://localhost:8134
cargo test                    # pruebas unitarias
cargo test -- --ignored ceuta # prueba real de recorte de mapas (requiere Internet)

# Imagen Docker (incluye la web)
docker build --platform linux/amd64 -t deca-server:0.2.0 .
docker run -d -p 8134:8134 -v deca-data:/data deca-server:0.2.0
```

Para desplegar con Docker Compose, ver [`../../git`](../../git).

| Variable                  | Por defecto | Uso |
|---------------------------|-------------|-----|
| `DECA_PORT`               | `8134`      | Puerto HTTP |
| `DECA_DATA_DIR`           | `./data` (`/data` en Docker) | BD, PDF, mapas |
| `DECA_WEB_DIR`            | `./web` (`/app/web` en Docker) | Carpeta de la aplicación web |
| `DECA_PUBLIC_URL`         | —           | URL pública HTTPS. Opcional: se detecta sola cuando un administrador entra por Cloudflare Tunnel |
| `DECA_ADMIN_PASSWORD`     | —           | Opcional, primer arranque: contraseña inicial de `Admin` en vez de `1234` (también `DECA_ADMIN_PASSWORD_FILE`) |
| `DECA_ALLOW_INSECURE_URL` | `false`     | Solo para pruebas locales: acepta `http://` |

## Viajes de ida y vuelta

Con `numero_viajes` = N se crean N DeCA (uno por viaje, cada uno con su QR). Los impares van A → B y los pares B → A:
en cada envío se intercambian origen y destino, y si hay varios envíos se recorren en orden inverso. Mercancía, peso y vehículo no cambian.
Los listados («Últimos 7 días», administración) muestran cada grupo en orden de viaje, y «DeCA más reciente» empieza por el viaje 1.

## Aplicación web (`web/`)

HTML + CSS + JavaScript (módulos ES) sin paso de compilación. El servidor la sirve en `/`, y la app Android lleva una copia dentro.
En la web, el servidor es la propia página; en Android, el que se elige al iniciar sesión.

Estética de señales de tráfico: botones con forma de placa de señal (azul informativa, verde de orientación, rojo de prohibición, amarillo de obras), paneles de chapa con tornillos, cabecera de asfalto con línea discontinua y el DeCA como billete troquelado con el QR. Los botones de continuar van en una barra fija abajo, siempre visibles.

```
web/
├── index.html, css/app.css      tema visual
├── js/app.js                    arranque, sesión y rutas
├── js/core/                     API, puente nativo (Android), enrutador, componentes, QR local, formatos
├── js/components/               billete y viajes, autocompletado, direcciones, selector de mapa
├── js/views/                    una pantalla por fichero (login, inicio, crear DeCA, ajustes...)
└── vendor/                      librerías y recursos de mapas incluidos (ver vendor/LICENSES.md)
```

Sin conexión, la web enseña los últimos DeCA guardados (el QR se genera en el dispositivo).

## Aplicación instalable en iPhone (PWA)

La misma web se instala desde Safari con «Añadir a pantalla de inicio», sin Mac, sin compilar y sin cuenta de Apple. Necesita entrar por `https://`.

- `/sw.js` lo genera el servidor (`src/routes/pwa.rs` + plantilla `src/pwa/sw.js`) con la lista de ficheros de `web/` y una huella de su contenido: al cambiar cualquier fichero, el navegador instala la versión nueva. Guarda la aplicación para abrir sin conexión y nunca intercepta `/api/` ni los enlaces públicos de los QR.
- `web/js/core/queue.js`: cola de DeCA sin conexión. En iPhone se envía con la aplicación abierta (iOS no da segundo plano a las webs): al abrirla, al recuperar la conexión y cada 30 s. Usa el mismo `client_ref`, así que no duplica DeCA.
- `web/js/core/localmaps.js`: mapas PMTiles guardados en el dispositivo (Cache API en iPhone, nativo en Android).
- Lectura de QR con `jsQR`, porque Safari no incluye `BarcodeDetector`.
- Sin widget y sin brillo máximo: Apple no los ofrece a las aplicaciones web.

Probado con WebKit 27.2 emulando un iPhone 15 Pro en modo instalado. Falta probarlo en un iPhone real (cámara, «Añadir a pantalla de inicio», descargas de PDF).

## Dirección pública y HTTPS (Cloudflare Tunnel)

No hace falta configurar nada. Cloudflare Tunnel recibe las peticiones por `https://` y las reenvía
al puerto 8134 añadiendo `X-Forwarded-Proto: https`, `Cf-Visitor` y el `Host` público (`src/public_url.rs`):

1. **Detección:** la primera vez que **un administrador** usa la app por la dirección del túnel, el servidor aprende esa dirección.
   Las peticiones de los trabajadores nunca la cambian: cualquiera puede escribir esas cabeceras.
2. **Confirmación:** el servidor se llama a sí mismo a través de ella (`GET https://<dominio>/verificacion/<código aleatorio>`),
   sin seguir redirecciones y leyendo como mucho 128 bytes. Solo si recibe el mismo código se guarda la dirección.
   Se vuelve a comprobar cada 30 minutos.
3. **Limitación:** solo se impide emitir cuando se sabe con certeza que el enlace no sería HTTPS (dirección `http://`)
   o cuando aún no se conoce ninguna dirección. Si la comprobación no se puede hacer (por ejemplo, sin Internet),
   el DeCA se emite con `aviso` y `enlace_verificado: false`.

Prioridad: dirección escrita en Ajustes -> Servidor > `DECA_PUBLIC_URL` > detectada y confirmada.
Para el QR de acceso de los trabajadores, si aún no se conoce la dirección pública, se usa la que está usando el admin (por ejemplo, la de la red local).

## Mapas sin conexión (como Organic Maps)

Funcionan igual que en Organic Maps:

| Organic Maps | DeCA Server |
|---|---|
| `countries.json` con el `id`, el tamaño `s` y el hash `h` de cada región, más una versión de datos | `GET /api/maps/catalogo`: regiones agrupadas con `version`, `tamano` y `sha256` |
| Un fichero `.mwm` por región en `maps/<versión>/<Región>.mwm` | Un fichero `.pmtiles` por región en `/api/maps/files/<versión>/<región>.pmtiles` |
| Descarga por trozos, reanudable (HTTP Range) | Igual (`Range` → 206) |
| Datos de OpenStreetMap | Datos de OpenStreetMap, a partir de la versión diaria vectorial de Protomaps |

Elegí PMTiles vectorial y no `.mwm` porque el formato `.mwm` solo lo puede dibujar el motor C++ de Organic Maps.
PMTiles es un estándar abierto que MapLibre dibuja sin conexión en Android, iOS y Flutter.

Regiones: las 17 comunidades autónomas, Ceuta, Melilla, Andorra y Portugal continental. Cuando alguien pulsa
«Descargar» en una región sin preparar, la app llama a `POST /api/maps/regiones/{id}/preparar`. El servidor la recorta del mapa mundial
de Protomaps (~140 GB) descargando solo las partes necesarias con HTTP Range (`src/maps/extract.rs`).
Prueba real: Comunidad de Madrid a zoom 14 → 60 MB en ~15 s. Ceuta a zoom 15 → 1,6 MB. El resultado lo validó la librería oficial `pmtiles`.
Las regiones se regeneran solas cuando pasan `maps_auto_update_days` días (30 por defecto).

Para dibujar el mapa, la app necesitará el estilo de [Protomaps basemaps](https://github.com/protomaps/basemaps) y sus fuentes y sprites
([basemaps-assets](https://github.com/protomaps/basemaps-assets)) incluidos en la propia app. Hay que mostrar la atribución «© OpenStreetMap contributors».

Prueba de extracción real (requiere Internet): `cargo test -- --ignored ceuta`.

## Cómo cumple la normativa

| Requisito | Implementación |
|---|---|
| Datos del art. 6 | `deca/model.rs`: cargador contractual = empresa cargadora (nombre, NIF y domicilio); transportista efectivo = empresa transportista (nombre, NIF y domicilio) + «Nombre transportista» (titular de los vehículos); origen, destino, mercancía, peso, volumen, fecha, matrículas y autorización especial |
| PDF nativo ≤ 5 MB con metadatos | `deca/pdf/`: generador PDF propio (texto real y QR vectorial). `CreationDate` y `ModDate` en el diccionario Info. Se comprueba el tamaño |
| QR visible en el PDF + QR suelto | El QR va dibujado en el PDF. También se sirve en `/api/decas/{id}/qr.png` y `qr.svg` |
| URL única `https://`, descarga directa sin login | `/deca/Año/Mes/Día/<token 256 bits>/<fichero>.pdf` devuelve el PDF sin pasos intermedios. HTTPS detectado y confirmado de ida y vuelta (ver arriba) |
| Generación atómica | Registro en BD, PDF y verificación dentro de una transacción. Si algo falla, no queda nada a medias |
| Enlace activo hasta fin de servicio + 7 días | `expires_at` = máx(emisión, fin del servicio a las 23:59) + 7 días. Después responde 410, pero el PDF se conserva |
| Conservación de 1 año | Descarga con sesión (`/api/decas/{id}/pdf`) aunque el enlace haya caducado. El borrado automático solo actúa pasados 366 días |
| Modificaciones (apartado Quinto, método 1) | Mismo PDF, misma URL y mismo QR. Los datos antiguos aparecen tachados con «NO VÁLIDO» y se indica el motivo. Cada versión anterior se guarda como `...-vN.pdf` |
| Varios envíos agrupados | `envios[]`, cada uno con su origen, destino, mercancía y peso |
| Registro de fecha y hora | La tabla `audit_log` guarda creación, modificación y cada descarga pública (controles) |

## Estructura del código

```
src/
├── main.rs            arranque del servidor
├── config.rs          variables de entorno
├── error.rs           errores → JSON con mensajes en español
├── state.rs           estado compartido + límite de intentos de login
├── auth/              sesiones Bearer, Argon2, TOTP (2FA de 6 dígitos)
├── db/                esquema SQLite, cuenta Admin inicial, ajustes del servidor
├── deca/
│   ├── model.rs       datos del DeCA, validación y cambios entre versiones
│   ├── pdf/           maquetación y escritor PDF
│   ├── qr.rs          QR (matriz, SVG, PNG)
│   ├── storage.rs     carpetas DeCA/Matrícula/Año/Mes/Día y nombres de fichero
│   ├── layout.rs      paso de los PDF de la estructura antigua (Año/Mes/Día) a la actual
│   └── mod.rs         emitir, modificar y consultar
├── preferences.rs     campos "fijados" (empresa, admin→trabajador, propios)
├── backup.rs          copias .json / .zip y restauración
├── jobs.rs            borrado diario a las 12:00 de PDF con más de 366 días
├── public_url.rs      dirección pública: detección por el túnel y confirmación HTTPS
├── maps/              mapas sin conexión por regiones (catálogo, recorte PMTiles) y teselas en caché
└── routes/            un módulo por apartado de la app
```

Los ficheros se guardan así: `DeCA/MatriculaDelCamion/Año/Mes/Día/NombreTrabajador-MatriculaDelCamion-Dia-Mes-Año-Hora-Minuto-Segundo.pdf`.
Si en el mismo segundo se emiten varios viajes, se añade `-2`, `-3`, etc.
La URL pública no lleva la matrícula (`/deca/Año/Mes/Día/<token>/<fichero>.pdf`), así que los QR
emitidos antes de este cambio siguen funcionando. Los PDF guardados con la estructura antigua
(`DeCA/Año/Mes/Día/…`) se mueven solos al arrancar y al restaurar una copia (`src/deca/layout.rs`).

## API

Todas las rutas `/api/*` (salvo `login` y `qr`) requieren `Authorization: Bearer <token>`.
Los errores siguen el formato `{"error": "mensaje para el usuario", "codigo": "..."}`.

### Sesión
| Método | Ruta | Descripción |
|---|---|---|
| POST | `/api/auth/login` | `{usuario, contrasena, codigo_2fa?}`. Si falta el código 2FA → 401 `codigo: "requiere_2fa"` |
| POST | `/api/auth/qr` | `{codigo}`: inicio automático con el QR que enseña el admin (un solo uso, válido 10 min) |
| POST | `/api/auth/logout` | |
| GET  | `/api/auth/me` | Usuario, empresa, 2FA y aviso de contraseña por defecto |
| GET  | `/api/setup/status` | `necesita_empresa`, `necesita_direccion_publica` (asistente inicial) |

El QR de acceso contiene `{"tipo":"deca-login","servidor":"https://...","usuario":"...","codigo":"..."}`.
La app lee `servidor` y llama a `POST {servidor}/api/auth/qr`.

### Ajustes del usuario
| Método | Ruta | Descripción |
|---|---|---|
| PUT | `/api/me` | `{nombre?, usuario?}` |
| PUT | `/api/me/password` | `{actual, nueva}` |
| POST | `/api/me/2fa/iniciar` | Devuelve secreto, `otpauth_url` y `qr_svg` |
| POST | `/api/me/2fa/activar` · `/desactivar` | `{codigo}` |
| GET/PUT | `/api/me/preferences` | Ajustes -> Preferencias. Rechaza los campos ya fijados por el admin |
| GET | `/api/me/preferences/effective` | Valores efectivos, con su origen y si están bloqueados |

Campos que se pueden fijar:

| Clave | Significado |
|---|---|
| `cargador_empresa`, `cargador_nif`, `cargador_domicilio` | Empresa cargadora: la que carga el camión |
| `transportista_empresa`, `transportista_nif`, `transportista_domicilio` | Empresa transportista: la que transporta. El NIF es el de esta empresa |
| `transportista_titular` | «Nombre transportista»: persona o sociedad dueña de los vehículos |
| `mercancia` | Por ejemplo, «Minerales a granel» |
| `volumen_m3`, `peso_kg` | Volumen de carga del camión y peso |
| `origen`, `destino` | Dirección completa |
| `matricula_camion`, `matricula_remolque` | Matrículas |
| `numero_viajes`, `conductor`, `autorizacion_especial` | Viajes (ida y vuelta), conductor opcional, autorización especial |

Prioridad: admin→trabajador > empresa > propias del trabajador > automáticas (los datos de la empresa, incluido su `titular`, como transportista, y el nombre del usuario como conductor).

### Empresa y trabajadores (admin)
| Método | Ruta | Apartado |
|---|---|---|
| GET/POST | `/api/companies` | Crear empresa (configuración inicial) |
| GET/PUT/DELETE | `/api/companies/{id}` | |
| GET/PUT | `/api/companies/{id}/preferences` | Administración de Preferencias -> Preferencias de la empresa |
| GET/POST | `/api/companies/{id}/workers` | Trabajadores en mi empresa / Crear cuenta trabajador `{usuario, nombre, contrasena}` |
| GET | `/api/companies/{id}/decas?trabajador=&desde=&hasta=&matricula=&pagina=` | Administración de DeCA emitidos |
| GET/PUT/DELETE | `/api/workers/{id}` | Perfil: cambiar `{usuario, nombre, contrasena, empresa_id, administrador}` o eliminar. Nadie puede cambiar su propio permiso ni eliminarse |
| DELETE | `/api/workers/{id}/2fa` | Quitar el 2FA del trabajador |
| GET/PUT | `/api/workers/{id}/preferences` | Gestionar preferencias del trabajador |
| POST | `/api/workers/{id}/qr-login` | «Enseñar código QR» |

### Base de datos compartida
`/api/catalog/{trucks|trailers|parties|places|goods}`: `GET ?q=` (y `tipo=cargador|transportista` en `parties`), `POST`, `PUT /{id}`, `DELETE /{id}`.
Las empresas (`parties`) aceptan `ubicaciones` (lista de direcciones completas) y las devuelven con `id`, `nombre` y `direccion`: al elegir una empresa en un DeCA se proponen como origen y destino.
Los camiones aceptan `remolque_id` o `remolque_matricula` como remolque predeterminado.
Al emitir un DeCA, los cargadores, transportistas, direcciones, mercancías, camiones y remolques nuevos se guardan solos en esta base de datos.

### DeCA
| Método | Ruta | Descripción |
|---|---|---|
| GET | `/api/decas/plantilla` | Formulario prerrellenado con las preferencias |
| POST | `/api/decas` | Emite: cuerpo de la plantilla + `numero_viajes` (un DeCA por viaje) + `client_ref` opcional (UUID: si se repite, devuelve los DeCA ya creados sin duplicarlos). Responde `{decas, enlace_verificado, aviso}` |
| GET | `/api/decas/ultimo` | «DeCA más reciente»: lista con todos los viajes del último DeCA, del 1 al último (vacía si no hay ninguno) |
| GET | `/api/decas/recientes` | «Últimos 7 días». El admin ve los de todos |
| GET/PUT | `/api/decas/{id}` | Detalle (incluye `viajes`: los demás viajes del mismo DeCA) / modificar `{datos, motivo}` |
| GET | `/api/decas/{id}/pdf[?version=N]` | PDF con sesión (conservación de 1 año) |
| GET | `/api/decas/{id}/qr.png[?escala=]` · `qr.svg` | QR suelto |
| GET | `/api/decas/{id}/historial` | Registro con fecha y hora |
| GET | `/api/widget/ultimo` | `{viajes: [...]}`: viajes del último DeCA con datos mínimos + QR en PNG base64 |
| GET | `/deca/...` | **Pública**: la URL del QR |
| GET | `/sw.js` | Service worker de la aplicación instalable (generado) |
| GET | `/api/info` | Comprueba que una dirección es un servidor DeCA (la usa la app al iniciar sesión) |

### Servidor (admin)
| Método | Ruta | Descripción |
|---|---|---|
| GET/PUT | `/api/server/settings` | `auto_delete_enabled` (Permitir eliminación automática), `public_base_url` (opcional), `public_link_days`, `maps_max_zoom`, `maps_auto_update_days`, `maps_source_url`, `maps_builds_url`, `tile_server_url`, `geocoder_url`. El GET incluye el estado de `direccion_publica` |
| POST | `/api/server/public-url/check` | Comprobar ahora el enlace público |
| GET | `/api/server/backup/config` | Copia de seguridad de configuración (.json) |
| GET | `/api/server/backup/full` | Copia de seguridad completa (.zip con los PDF) |
| POST | `/api/server/restore` | Multipart con el campo `archivo` (.json o .zip). Cierra todas las sesiones |
| POST | `/api/server/cleanup/run` | Ejecuta ahora la limpieza de PDF con más de 366 días |
| GET | `/api/server/status` | |

### Mapas (OpenStreetMap)
| Método | Ruta | Descripción |
|---|---|---|
| GET | `/api/maps/search?q=` · `/api/maps/reverse?lat=&lon=` | Búsqueda de direcciones (Nominatim) |
| GET | `/api/maps/catalogo` | Catálogo de regiones: estado (`no_preparada`, `en_cola`, `generando`, `lista`, `error`), progreso, versión, tamaño y SHA-256 |
| POST | `/api/maps/regiones/{id}/preparar` | Generar o actualizar una región (cualquier usuario) |
| DELETE | `/api/maps/regiones/{id}` | Borrar una región del servidor (admin) |
| GET | `/api/maps/files/{version}/{id}.pmtiles` | Descarga reanudable (`Range`). Cabecera `x-sha256` |
| GET | `/api/maps/tiles/{z}/{x}/{y}.png` | Teselas raster en caché para ver el mapa con conexión |

## Aviso

Esto no es asesoramiento legal. Antes de comercializar la aplicación, revisa el texto completo del BOE y las preguntas frecuentes del Ministerio.
