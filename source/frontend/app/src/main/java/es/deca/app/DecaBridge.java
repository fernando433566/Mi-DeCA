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
            if (PendingQueue.hasWork(ctx)) DecaSyncWorker.schedule(ctx, false);
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

    // ---------------- Widget y pantalla ----------------

    /** {"viajes": [...], "indice": n}: viajes del último DeCA y el que se muestra. */
    @JavascriptInterface
    public void updateWidget(String json) {
        try {
            JSONObject o = new JSONObject(json);
            JSONArray viajes = o.optJSONArray("viajes");
            DecaWidget.save(ctx, viajes == null ? "[]" : viajes.toString(), o.optInt("indice", 0));
        } catch (Exception e) {
            DecaWidget.save(ctx, "[]", 0);
        }
    }

    @JavascriptInterface
    public String widgetState() {
        return DecaWidget.state(ctx);
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
            DownloadManager.Request req = new DownloadManager.Request(Uri.parse(url))
                    .addRequestHeader("Authorization", "Bearer " + Session.token(ctx))
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
