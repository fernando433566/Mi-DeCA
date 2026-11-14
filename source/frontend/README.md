# DeCA · Frontend (app Android)

App Android (Java) que muestra la **misma aplicación web** del servidor (`../backend/web`) dentro de un
WebView. La web va incluida en la app, así que funciona sin conexión. Además añade funciones nativas a través
de un puente JavaScript (`window.DecaNative`, ver `../backend/web/js/core/native.js` ↔ `DecaBridge.java`).

## Funciones propias de Android

| Función | Cómo funciona |
|---|---|
| **DeCA sin conexión** | Si al emitir no hay conexión con el servidor, el DeCA se guarda en una cola (`PendingQueue`). WorkManager (`DecaSyncWorker`) lo envía en cuanto hay red y reintenta con espera creciente. El servidor no lo duplica gracias a `client_ref`. Al crearse, llega una notificación con el QR y se actualiza el widget. Mientras tanto, la app avisa de que el DeCA aún no es válido |
| **Widget** | Billete con el último DeCA y su QR (generado en el móvil). Se refresca cada 30 min y al emitir. Se añade desde Ajustes → «Poner el billete en la pantalla de inicio» |
| **Entrar con QR** | Escáner nativo (zxing-android-embedded) del QR que enseña el administrador |
| **Mapas sin conexión** | Ajustes → Mapas: descarga regiones PMTiles con el gestor de descargas (se reanuda si se corta) y comprueba su SHA-256. MapLibre las lee del móvil por trozos (`MapStore.read`) |
| **Enseñar QR** | Pantalla completa con el brillo al máximo para el control en carretera |
| **PDF de los DeCA** | Se descargan en los datos privados de la app (`files/pdf`, no en «Descargas») y se abren solos con el visor de PDF del móvil. Si ya están descargados se abren sin conexión («Abrir PDF»). Se borran solos cuando el DeCA sale de «Últimos 7 Días» (al abrir la app y cada 30 min) y al cambiar de cuenta (`PdfStore`, `FileProvider`) |
| **Actualizaciones** | `Updater`, `UpdateWorker`, `UpdateReceiver`: pregunta al servidor (`/api/app/update`) por la última versión del origen que eligió el administrador, descarga la APK (sin el token de la sesión), comprueba su SHA-256 si el origen lo da y la instala con `PackageInstaller` (sin preguntar cuando Android lo permite; si no, con su diálogo o una notificación). Automático al abrir la app y cada 12 h si el administrador lo activa; manual en Ajustes → «Versión de la app» |
| **Descargas** | Copias de seguridad en la carpeta «Descargas» (con el token de la sesión) |

## Compatibilidad: Android 8.0 (API 26) o superior

- `minSdk 26`. `lint` no detecta ninguna llamada a APIs posteriores (`NewApi`): los atributos del widget de Android 12+ (`targetCellWidth`, `previewLayout`) se ignoran en versiones anteriores.
- La interfaz se ejecuta en «Android System WebView» y necesita la **versión 94 o posterior** (la más exigente es MapLibre GL 6, que usa bloques `static` de clase y `Object.hasOwn`). Los móviles con Android 8 y Google Play reciben actualizaciones de WebView hasta la versión 138.
- Si el WebView es más antiguo (por ejemplo, un Android 8 sin actualizar, con el 69 de fábrica), la app no se queda en blanco: muestra un aviso con un botón a Google Play y se abre sola al volver una vez actualizado (`MainActivity.MIN_WEBVIEW`).
- Comprobado en un emulador Android 8.0.0 (API 26, imagen `google_apis` x86_64 r16, WebView 69): la APK de publicación se instala, arranca sin errores y muestra el aviso. La interfaz completa se ha probado en Android 15 (Samsung Galaxy Z Flip3, WebView 156) y en el emulador de la API 37.

## Estructura

```
app/src/main/
├── AndroidManifest.xml
├── java/es/deca/app/
│   ├── MainActivity.java         WebView, permisos, escáner, selector de ficheros, brillo
│   ├── WebAssets.java            sirve la web incluida en https://appassets.androidplatform.net/app/
│   ├── DecaBridge.java           funciones nativas llamadas desde JavaScript
│   ├── Session.java              servidor + token (para trabajar con la app cerrada)
│   ├── PendingQueue.java         cola de DeCA sin conexión (fichero JSON privado)
│   ├── DecaSyncWorker.java       envío de la cola con WorkManager
│   ├── Notifier.java             notificaciones (con el QR)
│   ├── DecaWidget.java           widget con flechas de viaje
│   ├── WidgetRefreshWorker.java  refresco periódico del widget
│   ├── MapStore.java             mapas descargados en el móvil
│   ├── QrBitmap.java             QR nativo (ZXing)
│   └── Http.java                 peticiones al servidor
└── res/                          widget (billete), iconos (señal), tema
```

## Compilar

Requisitos: SDK de Android (plataforma 37) y un JDK 17 o superior (sirve el `jbr` de Android Studio).

```bash
export JAVA_HOME=<android-studio>/jbr ANDROID_HOME=~/Android/Sdk
./gradlew assembleDebug     # app/build/outputs/apk/debug/app-debug.apk   (depuración, inspeccionable)
./gradlew assembleRelease   # app/build/outputs/apk/release/app-release.apk (firmada)
adb install -r app/build/outputs/apk/release/app-release.apk
```

Antes de compilar, Gradle copia `../backend/web` dentro de la app (tarea `copyWeb`), así que cualquier cambio en la web llega a la web y a la app.

**Firma de la versión de publicación.** La clave **no** está en el repositorio. `assembleRelease` la busca en
`../../../secrets/keystore.properties` (o en la ruta de la variable `DECA_KEYSTORE_PROPERTIES`):

```properties
storeFile=deca-release.jks
storePassword=...
keyAlias=deca
keyPassword=...
```

Para crear una clave nueva:

```bash
keytool -genkeypair -keystore deca-release.jks -storetype PKCS12 -alias deca \
  -keyalg RSA -keysize 4096 -validity 10000 -dname "CN=DeCA, OU=DeCA App, C=ES"
```

Guarda una copia de la clave en lugar seguro: sin ella no se pueden publicar actualizaciones que se instalen encima de la versión anterior.

Versiones: Gradle 9.8.2, Android Gradle Plugin 9.4.1, compileSdk/targetSdk 37, minSdk 26 (Android 8.0).
Dependencias: androidx core 1.19.1, activity 1.13.0, webkit 1.17.1, work 2.12.0 y zxing-android-embedded 4.3.0.

La app permite servidores `http://` de la red local además de `https://`. Los QR públicos deben ser siempre `https://`.
