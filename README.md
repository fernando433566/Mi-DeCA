# DeCA · Versión compilada (lista para instalar)

| Fichero | Qué es |
|---|---|
| `deca-server-0.1.0-amd64.tar.gz` | Imagen Docker x86_64 del servidor: backend Rust + aplicación web del puerto 8134 |
| `docker-compose.yml` | Despliegue: imagen, puertos, volumen de datos y enlace HTTPS |
| `.env.example` | Variables que se pueden cambiar (copiar como `.env`) |
| `DeCA-0.1.0.apk` | App Android firmada (versión de publicación), Android 8.0 o superior |
| `SHA256SUMS` | Sumas de comprobación de los ficheros anteriores |

El código fuente está en [`../source`](../source) y la descripción completa del proyecto en el [README principal](../README.md).

## 1. Servidor

Requisitos: un equipo Linux x86_64 con Docker y Docker Compose.

```bash
sha256sum -c SHA256SUMS                         # comprobar los ficheros
docker load -i deca-server-0.1.0-amd64.tar.gz   # carga la imagen deca-server:0.1.0
cp .env.example .env                            # y editar los valores (ver abajo)
docker compose up -d
docker compose ps                               # debe aparecer "healthy"
```

Abre `http://<ip-del-equipo>:8134` y entra con **`Admin` / `1234`**. Un asistente te obliga a cambiar la contraseña y a crear la empresa.

### Variables de `.env`

| Variable | Por defecto | Para qué |
|---|---|---|
| `DECA_IMAGE` | `deca-server:0.1.0` | Imagen. Si la subes a un registro (por ejemplo `ghcr.io/<usuario>/deca-server:0.1.0`), con esto se descarga con `docker compose pull` |
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
docker tag deca-server:0.1.0 ghcr.io/<usuario>/deca-server:0.1.0
docker login ghcr.io && docker push ghcr.io/<usuario>/deca-server:0.1.0
```

Después, en otros equipos basta con poner ese nombre en `DECA_IMAGE` y ejecutar `docker compose pull`.

## 2. App Android

Copia `DeCA-0.1.0.apk` al móvil y ábrela (hay que permitir «instalar aplicaciones de origen desconocido»), o desde un ordenador:

```bash
adb install DeCA-0.1.0.apk
```

- **Compatible con Android 8.0 (API 26) o superior.** La interfaz necesita «Android System WebView» 94 o posterior, que los móviles con Google Play reciben como actualización. Si es más antiguo, la app lo explica y lleva a Google Play para actualizarlo.
- Al abrirla: «Entrar con código QR» (el administrador lo enseña en Ajustes → empresa → Trabajadores → perfil) o escribir la dirección del servidor, el usuario y la contraseña.
- Para tener el widget con el último DeCA: Ajustes → «Poner el billete en la pantalla de inicio».

Certificado de firma de la APK (SHA-256), para comprobar que una actualización viene del mismo origen
(`apksigner verify --print-certs DeCA-0.1.0.apk`):

```
3a287175f979a4efeacd24fb25382474ecb338ff6086e58f9c7df9337cdbf6b9
```
