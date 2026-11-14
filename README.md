Esta app fue creada con IA. No me hago responsable de daños por errores, uso indevido o cualquier cosa relacionada con la app, use este programa bajo su propio riesgo
# DeCA · Versión compilada (lista para instalar)

| Fichero | Qué es |
|---|---|
| `deca-server-0.3.0-amd64.tar.gz` | Imagen Docker x86_64 del servidor: backend Rust + aplicación web del puerto 8134. También publicada en `ghcr.io/fernando433566/deca-server:latest` |
| `docker-compose.yml` | Despliegue: imagen, puertos, volumen de datos y enlace HTTPS |
| `.env.example` | Variables que se pueden cambiar (copiar como `.env`) |
| `DeCA-0.3.0.apk` | App Android firmada (versión de publicación), Android 8.0 o superior |
| `SHA256SUMS` | Sumas de comprobación de los ficheros anteriores |

El código fuente está en [`../source`](../source) y la descripción completa del proyecto en el [README principal](../README.md).

## 1. Servidor

Requisitos: un equipo Linux x86_64 con Docker y Docker Compose.

```bash
sha256sum -c SHA256SUMS                         # comprobar los ficheros
docker load -i deca-server-0.3.0-amd64.tar.gz   # carga la imagen deca-server:0.3.0, tambien se puede hacer docker pull a la imagen si se prefiere
cp .env.example .env                            # y editar los valores (ver abajo)
docker compose up -d
docker compose ps                               # debe aparecer "healthy"
```
Abre `http://<ip-del-equipo>:8134` y entra con **`Admin` / `1234`**. Un asistente te obliga a cambiar la contraseña y a crear la empresa: hasta cambiarla, el servidor no deja hacer nada más con esa cuenta. Si prefieres que la contraseña inicial no sea `1234`, escribe una de 12 caracteres o más en `DECA_ADMIN_PASSWORD` (en `.env`) antes del primer arranque.

### Variables de `.env`

| Variable | Por defecto | Para qué |
|---|---|---|
| `DECA_IMAGE` | `deca-server:0.3.0` | Imagen. Se puede descargar ya compilada o a través de docker pull` |
| `DECA_PUBLIC_URL` | vacío | Enlace **HTTPS** de Cloudflare Tunnel con el que se accede desde fuera y que va en los QR. Si se deja vacío, se detecta y se comprueba sola la primera vez que **un administrador** entra por el túnel |
| `DECA_BIND_ADDRESS` | `0.0.0.0` | `0.0.0.0` = accesible desde la red local; `127.0.0.1` = solo desde este equipo (si solo se entra por el túnel) |
| `DECA_HOST_PORT` | `8134` | Puerto en el equipo anfitrión. Dentro del contenedor siempre es 8134 |
| `DECA_ADMIN_PASSWORD` | vacío | Opcional, solo en el primer arranque: contraseña inicial de `Admin` (12+ caracteres) en lugar de `1234` |
| `DECA_DATA` | `deca-data` | Dónde se guardan la base de datos, los PDF (`DeCA/Matrícula/Año/NombreMes/Día/Nº<n>-….pdf`) y los mapas: el volumen de Docker `deca-data` o la ruta absoluta de una carpeta propiedad del UID 10001 (`sudo chown -R 10001 /ruta`) |

### Acceso remoto (Cloudflare Tunnel)

El túnel se instala aparte, no está en el contenedor. Apúntalo a `http://localhost:8134` (o al puerto de `DECA_HOST_PORT`), por ejemplo:

```bash
cloudflared tunnel --url http://localhost:8134                # prueba rápida (dirección *.trycloudflare.com)
# o un túnel con nombre y tu dominio: deca.tu-dominio.es -> http://localhost:8134
```

Luego entra una vez como administrador por esa dirección `https://`. En Ajustes → Servidor debe aparecer «Comprobado por HTTPS».

### Mantenimiento

```bash
docker compose logs -f              # registro
docker compose pull && docker compose up -d   # actualizar (si usas un registro)
```

Las copias de seguridad se hacen desde la propia aplicación: Ajustes → Servidor → Crear copia de seguridad.

## 2. App Android

se instala como cualquier otra app

## 3. iPhone (aplicación instalable)

No hay fichero que instalar ni hace falta Mac, cuenta de Apple ni pagar nada: la aplicación se añade desde Safari.

1. Abre en Safari la dirección **`https://`** del servidor (la de Cloudflare Tunnel). Por `http://` de la red local no se puede instalar: Apple exige conexión segura.
2. Pulsa **Compartir** → **«Añadir a pantalla de inicio»**. Si aparece «Abrir como app web», déjalo activado.
3. Abre DeCA desde el icono e inicia sesión: la aplicación instalada tiene su propia sesión, distinta de la de Safari.

La propia aplicación enseña estos pasos en la pantalla de inicio de sesión. No caduca y no tiene límite de móviles.

| Función | Android | iPhone |
|---|---|---|
| Crear DeCA, QR, últimos 7 días, ajustes | Sí | Sí |
| Abrir y enseñar los DeCA sin conexión | Sí | Sí |
| DeCA creados sin conexión | Se envían solos, aunque la app esté cerrada | Se envían con la aplicación **abierta** y con cobertura |
| Aviso cuando el DeCA pendiente ya tiene QR | Notificación | Mensaje dentro de la aplicación |
| Entrar con código QR | Sí | Sí (cámara) |
| Mapas por regiones sin conexión | Sí, se reanuda si se corta | Sí, con la aplicación abierta durante la descarga |
| Widget | Sí | No: Apple no lo permite a las aplicaciones web |
| Brillo al máximo al enseñar el QR | Sí | No: solo evita que se apague la pantalla |

## 4. Acceso por la red local (HTTP)

La aplicación web se abre sin HTTPS desde cualquier equipo de la red local: `http://<ip-del-equipo>:8134` (con `DECA_BIND_ADDRESS=0.0.0.0`, el valor por defecto).
Se puede entrar, crear DeCA y usar los ajustes. Los QR siguen llevando la dirección `https://` pública, porque es la que exige la normativa: si todavía no se conoce ninguna, hay que entrar una vez por el túnel o escribirla en Ajustes → Servidor antes de emitir.
Los navegadores solo permiten instalar la aplicación, usarla sin conexión y leer QR con la cámara por `https://`.

## 5. Administradores

Por defecto, las cuentas nuevas son de trabajador. Un administrador puede escalarlas en Ajustes → empresa → Trabajadores → Trabajadores en mi empresa → perfil → casilla **Administrador**. Nadie puede cambiarse su propio permiso, y la casilla no aparece a los trabajadores.

## Cambios de la versión 0.3.0

- Se elimina la emisión de varios viajes de ida y vuelta: cada emisión crea un DeCA. Los ya emitidos así siguen existiendo, cada uno por separado.
- «Últimos 7 Días» muestra billetes reducidos con un QR pequeño. Al pulsar uno se abre la página del DeCA o, si está activada la preferencia visual «Mostrar PDF directamente al seleccionar un DeCA», su PDF.
- Nuevo apartado «Preferencias visuales» en las preferencias de cada usuario. El administrador puede fijar cada ajuste como activado o desactivado por defecto para la empresa o para un trabajador; entonces el trabajador no puede cambiarlo.
- Actualizaciones de la app Android: en Ajustes → Servidor → «Actualizaciones de la app Android» el administrador decide si la app busca e instala sola las versiones nuevas (al abrirse y cada 12 h) para él y todos sus trabajadores, y de dónde: por defecto las releases de https://github.com/fernando433566/Mi-DeCA/releases, o la dirección que ponga (releases de GitHub o Gitea/Forgejo/Codeberg, o un JSON `{"version", "apk", "sha256"}`), como Obtainium. Cualquiera puede buscar a mano en Ajustes → «Versión de la app». La release debe incluir la APK; Android solo la instala si está firmada con la misma clave. La primera vez, Android pide permitir que DeCA instale aplicaciones; a partir de la segunda actualización suele instalarse sin preguntar.
- Android: los PDF se descargan en los datos de la app (no en «Descargas»), se abren al terminar de descargarse y, si ya están descargados, se abren directamente («Abrir PDF»), también sin conexión. Se borran del móvil cuando el DeCA sale de «Últimos 7 Días». En el navegador y en iPhone el PDF se abre en una pestaña nueva.
- Los PDF se llaman `Nº<número>-Trabajador-Matrícula-Día-NombreMes-Año-Hora-Minuto-Segundo.pdf` (1 el primero emitido, 2 el segundo…) y la carpeta del mes lleva su nombre: `DeCA/1234BCD/2026/Noviembre/14/Nº7-Pepe_Perez-1234BCD-14-Noviembre-2026-13-05-09.pdf`. Los PDF ya guardados se renombran y se mueven solos al arrancar; los QR ya emitidos siguen funcionando.

## Cambios de la versión 0.2.2

- Los PDF se guardan en `DeCA/Matrícula/Año/Mes/Día/`. Al arrancar (y al restaurar una copia antigua) los PDF ya existentes se mueven solos a esa estructura; los enlaces de los QR no cambian.

## Cambios de seguridad de la versión 0.2.1

- Con la contraseña inicial `1234` el administrador solo puede cambiarla; el resto de la API responde «Cambia la contraseña inicial antes de continuar».
- Cambiar una contraseña cierra las demás sesiones y anula los QR de acceso pendientes. Un administrador que cambia la de un trabajador le cierra todas.
- Para cambiar la verificación en dos pasos hay que desactivar primero la actual con su código.
- La dirección pública solo se aprende de las peticiones de un administrador (antes podía cambiarla cualquier trabajador manipulando cabeceras).
- Los PDF conservan su nombre (`Trabajador-Matrícula-fecha-hora.pdf`); el nombre se reserva en disco para que dos emisiones simultáneas no se pisen.
- Android separa los DeCA pendientes por servidor y usuario. Los que estuvieran pendientes al actualizar pasan a la cuenta con la sesión abierta.
- El contenedor se ejecuta con sistema de ficheros de solo lectura (salvo `/data` y `/tmp`) y sin capacidades.
