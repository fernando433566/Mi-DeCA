package es.deca.app;

import android.content.Context;
import android.webkit.WebResourceResponse;

import androidx.annotation.NonNull;
import androidx.webkit.WebViewAssetLoader;

import java.io.InputStream;
import java.util.HashMap;
import java.util.Map;

/**
 * Sirve la aplicación web incluida en la app (assets/web) en
 * https://appassets.androidplatform.net/app/... con el tipo MIME correcto
 * (los módulos .mjs de MapLibre no cargan si el tipo no es JavaScript).
 */
public class WebAssets implements WebViewAssetLoader.PathHandler {
    private static final Map<String, String> MIME = new HashMap<>();

    static {
        MIME.put("html", "text/html");
        MIME.put("js", "text/javascript");
        MIME.put("mjs", "text/javascript");
        MIME.put("css", "text/css");
        MIME.put("json", "application/json");
        MIME.put("webmanifest", "application/manifest+json");
        MIME.put("svg", "image/svg+xml");
        MIME.put("png", "image/png");
        MIME.put("pbf", "application/x-protobuf");
        MIME.put("txt", "text/plain");
    }

    private final Context ctx;

    WebAssets(Context ctx) {
        this.ctx = ctx;
    }

    @Override
    public WebResourceResponse handle(@NonNull String path) {
        if (path.isEmpty() || path.endsWith("/")) path += "index.html";
        if (path.contains("..")) return null;
        try {
            InputStream in = ctx.getAssets().open("web/" + path);
            String ext = path.substring(path.lastIndexOf('.') + 1).toLowerCase();
            String mime = MIME.getOrDefault(ext, "application/octet-stream");
            WebResourceResponse r = new WebResourceResponse(mime, "utf-8", in);
            Map<String, String> h = new HashMap<>();
            h.put("Access-Control-Allow-Origin", "*");
            h.put("Cache-Control", "no-cache");
            r.setResponseHeaders(h);
            return r;
        } catch (Exception e) {
            // Fichero interno inexistente: 404 (sin intentar salir a la red).
            WebResourceResponse r = new WebResourceResponse("text/plain", "utf-8", new java.io.ByteArrayInputStream(new byte[0]));
            r.setStatusCodeAndReasonPhrase(404, "Not Found");
            return r;
        }
    }
}
