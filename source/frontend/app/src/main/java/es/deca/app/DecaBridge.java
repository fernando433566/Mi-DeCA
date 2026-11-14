package es.deca.app;

import android.app.DownloadManager;
import android.content.Context;
import android.net.ConnectivityManager;
import android.net.NetworkCapabilities;
import android.net.Uri;
import android.os.Environment;
import android.webkit.JavascriptInterface;
import android.widget.Toast;

import org.json.JSONArray;
import org.json.JSONObject;

/**
 * Funciones nativas que la parte web llama como `window.DecaNative.metodo(...)`
 * (ver web/js/core/native.js).
 */
public class DecaBridge {
    private final MainActivity activity;
    private final Context ctx;

    DecaBridge(MainActivity activity) {
        this.activity = activity;
        this.ctx = activity.getApplicationContext();
    }

    @JavascriptInterface
    public void setSession(String json) {
        Session.save(ctx, json);
        if (Session.active(ctx)) {
            WidgetRefreshWorker.schedule(ctx);
            UpdateWorker.schedule(ctx);
            if (PendingQueue.hasWork(ctx)) DecaSyncWorker.schedule(ctx, true);
        }
    }

    @JavascriptInterface
    public String appVersion() {
        return BuildConfig.VERSION_NAME;
    }

    @JavascriptInterface
    public boolean isOnline() {
        ConnectivityManager cm = ctx.getSystemService(ConnectivityManager.class);
        NetworkCapabilities caps = cm.getNetworkCapabilities(cm.getActiveNetwork());
        return caps != null && caps.hasCapability(NetworkCapabilities.NET_CAPABILITY_INTERNET);
    }

    // ---------------- Escáner QR ----------------

    @JavascriptInterface
    public void scanQr(String callbackId) {
        activity.runOnUiThread(() -> activity.startQrScan(callbackId));
    }

    // ---------------- Cola sin conexión ----------------

    @JavascriptInterface
    public String enqueueDeca(String payloadJson) {
        try {
            JSONObject item = PendingQueue.add(ctx, new JSONObject(payloadJson));
            DecaSyncWorker.schedule(ctx, false);
            return item.toString();
        } catch (Exception e) {
            return null;
        }
    }

    @JavascriptInterface
    public String pendingDecas() {
        return PendingQueue.all(ctx).toString();
    }

    @JavascriptInterface
    public void removePending(String id) {
        PendingQueue.remove(ctx, id);
    }

    @JavascriptInterface
    public void retryPending() {
        DecaSyncWorker.schedule(ctx, true);
    }

    // ---------------- PDF de los DeCA (datos privados de la app) ----------------

    /**
     * Abre el PDF de un DeCA con el visor del móvil; si no está descargado, lo descarga
     * antes en los datos de la app (ver {@link PdfStore}). `infoJson`: {id, version, creado, archivo}.
     */
    @JavascriptInterface
    public void openPdf(String callbackId, String infoJson) {
        new Thread(() -> {
            try {
                Session.Snapshot owner = Session.snapshot(ctx);
                if (!owner.active()) throw new IllegalStateException("Inicia sesión para ver el PDF");
                java.io.File pdf = PdfStore.fetch(ctx, owner, PdfStore.Info.parse(infoJson));
                activity.runOnUiThread(() -> {
                    try {
                        PdfStore.open(activity, pdf);
                        activity.resolveJs(callbackId, true, "");
                    } catch (Exception e) {
                        activity.resolveJs(callbackId, false, e.getMessage());
                    }
                });
            } catch (Exception e) {
                String msg = e.getMessage() != null ? e.getMessage() : "No se pudo abrir el PDF";
                activity.runOnUiThread(() -> activity.resolveJs(callbackId, false, msg));
            }
        }).start();
    }

    /** ¿Está ya descargado este PDF en el móvil? */
    @JavascriptInterface
    public boolean pdfSaved(String infoJson) {
        try {
            return PdfStore.saved(ctx, Session.snapshot(ctx), PdfStore.Info.parse(infoJson));
        } catch (Exception e) {
            return false;
        }
    }

    // ---------------- Actualizaciones de la app ----------------

    /**
     * Ajustes → «Buscar actualizaciones»: comprueba ahora y, si hay versión nueva, la instala.
     * Resuelve con "al_dia" o "instalando"; los errores llegan con su mensaje.
     */
    @JavascriptInterface
    public void checkUpdate(String callbackId) {
        new Thread(() -> {
            String value;
            boolean ok = true;
            try {
                value = Updater.checkAndInstall(ctx, true) == Updater.Result.INSTALLING ? "instalando" : "al_dia";
            } catch (Exception e) {
                ok = false;
                value = e.getMessage() != null ? e.getMessage() : "No se pudo buscar actualizaciones";
            }
            boolean result = ok;
            String message = value;
            activity.runOnUiThread(() -> activity.resolveJs(callbackId, result, message));
        }).start();
    }

    // ---------------- Widget y pantalla ----------------

    /** Último DeCA (JSON de la API) para el widget, o "null" para vaciarlo. */
    @JavascriptInterface
    public void updateWidget(String json) {
        try {
            DecaWidget.save(ctx, json == null || json.equals("null") ? null : new JSONObject(json));
        } catch (Exception e) {
            DecaWidget.save(ctx, null);
        }
    }

    /** Pide al launcher que añada el widget a la pantalla de inicio. */
    @JavascriptInterface
    public boolean pinWidget() {
        android.appwidget.AppWidgetManager m = android.appwidget.AppWidgetManager.getInstance(ctx);
        if (!m.isRequestPinAppWidgetSupported()) return false;
        // Vista previa ya dibujada para el diálogo del launcher.
        android.os.Bundle extras = new android.os.Bundle();
        extras.putParcelable(android.appwidget.AppWidgetManager.EXTRA_APPWIDGET_PREVIEW, DecaWidget.render(ctx));
        return m.requestPinAppWidget(new android.content.ComponentName(ctx, DecaWidget.class), extras, null);
    }

    @JavascriptInterface
    public void setBrightness(boolean max) {
        activity.runOnUiThread(() -> activity.setMaxBrightness(max));
    }

    // ---------------- Descargas (PDF, copias de seguridad) ----------------

    @JavascriptInterface
    public void download(String url, String fileName, String mime) {
        try {
            Session.Snapshot owner = Session.snapshot(ctx);
            if (!owner.active() || fileName == null || fileName.contains("/") || fileName.contains("\\")) {
                throw new IllegalArgumentException("Nombre de descarga no válido");
            }
            String destination = ServerUrl.authenticated(owner.server, url);
            DownloadManager.Request req = new DownloadManager.Request(Uri.parse(destination))
                    .addRequestHeader("Authorization", "Bearer " + owner.token)
                    .setTitle(fileName)
                    .setNotificationVisibility(DownloadManager.Request.VISIBILITY_VISIBLE_NOTIFY_COMPLETED)
                    .setDestinationInExternalPublicDir(Environment.DIRECTORY_DOWNLOADS, fileName);
            if (mime != null && !mime.isEmpty()) req.setMimeType(mime);
            ctx.getSystemService(DownloadManager.class).enqueue(req);
            activity.runOnUiThread(() -> Toast.makeText(ctx, "Descargando " + fileName + " en «Descargas»", Toast.LENGTH_LONG).show());
        } catch (Exception e) {
            activity.runOnUiThread(() -> Toast.makeText(ctx, "No se pudo descargar: " + e.getMessage(), Toast.LENGTH_LONG).show());
        }
    }

    // ---------------- Mapas sin conexión ----------------

    @JavascriptInterface
    public String mapsLocal() {
        return MapStore.list(ctx).toString();
    }

    @JavascriptInterface
    public void mapDownload(String regionJson) {
        try {
            MapStore.download(ctx, new JSONObject(regionJson));
        } catch (Exception e) {
            activity.runOnUiThread(() -> Toast.makeText(ctx, "No se pudo descargar el mapa", Toast.LENGTH_LONG).show());
        }
    }

    @JavascriptInterface
    public void mapDelete(String id) {
        MapStore.delete(ctx, id);
    }

    @JavascriptInterface
    public String mapRead(String id, double offset, double length) {
        return MapStore.read(ctx, id, (long) offset, (int) length);
    }

    static String jsString(String s) {
        return JSONObject.quote(s == null ? "" : s);
    }

    @SuppressWarnings("unused")
    static JSONArray empty() {
        return new JSONArray();
    }
}
