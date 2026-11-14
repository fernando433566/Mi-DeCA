package es.deca.app;

import java.io.ByteArrayOutputStream;
import java.io.IOException;
import java.io.InputStream;
import java.io.OutputStream;
import java.net.HttpURLConnection;
import java.net.URL;
import java.nio.charset.StandardCharsets;

/** Peticiones HTTP sencillas al servidor DeCA (con el token de la sesión). */
public final class Http {
    private Http() {}

    public static final class Response {
        public final int status;
        public final String body;

        Response(int status, String body) {
            this.status = status;
            this.body = body;
        }

        public boolean ok() {
            return status >= 200 && status < 300;
        }
    }

    /**
     * Descarga un fichero binario en `dest` (primero en un temporal; si algo falla no queda
     * nada a medias). Devuelve la respuesta: si no es correcta, `body` trae el error del servidor.
     * Lanza IOException si no hay conexión con el servidor.
     * Solo sigue redirecciones si no lleva token (descargas públicas, p. ej. una APK de GitHub):
     * así el token de la sesión nunca se envía a otro sitio.
     */
    public static Response download(String url, String token, java.io.File dest, long maxBytes) throws IOException {
        HttpURLConnection con = (HttpURLConnection) new URL(url).openConnection();
        java.io.File tmp = new java.io.File(dest.getParentFile(), dest.getName() + ".part");
        try {
            con.setInstanceFollowRedirects(token == null || token.isEmpty());
            con.setConnectTimeout(15000);
            con.setReadTimeout(60000);
            if (token != null && !token.isEmpty()) con.setRequestProperty("Authorization", "Bearer " + token);
            int status = con.getResponseCode();
            if (status != 200) {
                InputStream err = con.getErrorStream();
                String body = "";
                if (err != null) {
                    try (InputStream is = err) {
                        byte[] buf = new byte[4096];
                        int n = is.read(buf);
                        if (n > 0) body = new String(buf, 0, n, StandardCharsets.UTF_8);
                    }
                }
                return new Response(status, body);
            }
            try (InputStream is = con.getInputStream(); OutputStream os = new java.io.FileOutputStream(tmp)) {
                byte[] buf = new byte[8192];
                long total = 0;
                int n;
                while ((n = is.read(buf)) > 0) {
                    total += n;
                    if (total > maxBytes) throw new IOException("Fichero demasiado grande");
                    os.write(buf, 0, n);
                }
            }
            if (!tmp.renameTo(dest)) throw new IOException("No se pudo guardar el fichero");
            return new Response(status, "");
        } finally {
            //noinspection ResultOfMethodCallIgnored
            tmp.delete();
            con.disconnect();
        }
    }

    /** Lanza IOException si no hay conexión con el servidor. */
    public static Response request(String method, String url, String token, String jsonBody) throws IOException {
        HttpURLConnection con = (HttpURLConnection) new URL(url).openConnection();
        try {
        con.setInstanceFollowRedirects(false);
        con.setConnectTimeout(15000);
        con.setReadTimeout(60000);
        con.setRequestMethod(method);
        con.setRequestProperty("Accept", "application/json");
        if (token != null && !token.isEmpty()) con.setRequestProperty("Authorization", "Bearer " + token);
        if (jsonBody != null) {
            con.setDoOutput(true);
            con.setRequestProperty("Content-Type", "application/json; charset=utf-8");
            try (OutputStream os = con.getOutputStream()) {
                os.write(jsonBody.getBytes(StandardCharsets.UTF_8));
            }
        }
        int status = con.getResponseCode();
        InputStream in = status >= 400 ? con.getErrorStream() : con.getInputStream();
        String body = "";
        if (in != null) {
            try (InputStream is = in) {
                ByteArrayOutputStream out = new ByteArrayOutputStream();
                byte[] buf = new byte[8192];
                int n;
                while ((n = is.read(buf)) > 0) {
                    if (out.size() + n > 16 * 1024 * 1024) throw new IOException("Respuesta demasiado grande");
                    out.write(buf, 0, n);
                }
                body = out.toString("UTF-8");
            }
        }
        return new Response(status, body);
        } finally { con.disconnect(); }
    }
}
