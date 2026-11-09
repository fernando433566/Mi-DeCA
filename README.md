Esta app fue creada con IA. No me hago responsable de daños por errores, uso indevido o cualquier cosa relacionada con la app, use este programa bajo su propio riesgo
# DeCA · Versión compilada (lista para instalar)

| Fichero | Qué es |
|---|---|
| `deca-server-0.2.0-amd64.tar.gz` | Imagen Docker x86_64 del servidor: backend Rust + aplicación web del puerto 8134. También publicada en `ghcr.io/fernando433566/deca-server:latest` |
| `docker-compose.yml` | Despliegue: imagen, puertos, volumen de datos y enlace HTTPS |
| `.env.example` | Variables que se pueden cambiar (copiar como `.env`) |
| `DeCA-0.2.0.apk` | App Android firmada (versión de publicación), Android 8.0 o superior |
| `SHA256SUMS` | Sumas de comprobación de los ficheros anteriores |

El código fuente está en [`../source`](../source) y la descripción completa del proyecto en el [README principal](../README.md).

## 1. Servidor

Requisitos: un equipo Linux x86_64 con Docker y Docker Compose.

```bash
sha256sum -c SHA256SUMS                         # comprobar los ficheros
docker load -i deca-server-0.2.0-amd64.tar.gz   # carga la imagen deca-server:0.2.0, tambien se puede hacer docker pull a la imagen si se prefiere
cp .env.example .env                            # y editar los valores (ver abajo)
docker compose up -d
docker compose ps                               # debe aparecer "healthy"
```
Abre `http://<ip-del-equipo>:8134` y entra con **`Admin` / `1234`**. Un asistente te obliga a cambiar la contraseña y a crear la empresa.

### Variables de `.env`

| Variable | Por defecto | Para qué |
|---|---|---|
| `DECA_IMAGE` | `deca-server:0.2.0` | Imagen. Si la subes a un registro (por ejemplo `ghcr.io/<usuario>/deca-server:0.2.0`), con esto se descarga con `docker compose pull` |
| `DECA_PUBLIC_URL` | vacío | Enlace **HTTPS** de Cloudflare Tunnel con el que se accede desde fuera y que va en los QR. Si se deja vacío, se detecta y se comprueba solo la primera vez que alguien entra por el túnel |
| `DECA_BIND_ADDRESS` | `0.0.0.0` | `0.0.0.0` = accesible desde la red local; `127.0.0.1` = solo desde este equipo (si solo se entra por el túnel) |
| `DECA_HOST_PORT` | `8134` | Puerto en el equipo anfitrión. Dentro del contenedor siempre es 8134 |
| `DECA_DATA` | `deca-data` | Dónde se guardan la base de datos, los PDF (`DeCA/Año/Mes/Día/…`) y los mapas: el volumen de Docker `deca-data` o la ruta absoluta de una carpeta propiedad del UID 10001 (`sudo chown -R 10001 /ruta`) |

### Acceso remoto (Cloudflare Tunnel)

El túnel se instala aparte, no está en el contenedor. Apúntalo a `http://localhost:8134` (o al puerto de `DECA_HOST_PORT`), por ejemplo:

```bash
cloudflared tunnel --url http://localhost:8134                # prueba rápida (dirección *.trycloudflare.com)
# o un túnel con nombre y tu dominio: deca.tu-dominio.es -> http://localhost:8134
```

Luego entra una vez por esa dirección `https://`. En Ajustes → Servidor debe aparecer «Comprobado por HTTPS».

### Mantenimiento

```bash
docker compose logs -f              # registro
docker compose pull && docker compose up -d   # actualizar (si usas un registro)
```

Las copias de seguridad se hacen desde la propia aplicación: Ajustes → Servidor → Crear copia de seguridad.

### Publicar la imagen en un registro (opcional)

```bash
docker tag deca-server:0.2.0 ghcr.io/<usuario>/deca-server:0.2.0
docker login ghcr.io && docker push ghcr.io/<usuario>/deca-server:0.2.0
```

Después, en otros equipos basta con poner ese nombre en `DECA_IMAGE` y ejecutar `docker compose pull`.

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
