package es.deca.app;

import android.content.ActivityNotFoundException;
import android.content.Context;
import android.content.Intent;
import android.net.Uri;

import androidx.core.content.FileProvider;

import org.json.JSONObject;

import java.io.File;
import java.io.IOException;
import java.time.Instant;

/**
 * PDF de DeCA descargados en los datos privados de la app (`files/pdf`), no en «Descargas»:
 * Android los cuenta como datos de la app y se borran al desinstalarla.
 *
 * Estructura: `pdf/<cuenta>/<id>-v<versión>-<emisión en segundos>/<nombre del PDF>`
 * - `<cuenta>`: servidor + usuario ({@link Session.Snapshot#key()}): cada cuenta ve solo los suyos,
 *   y al cambiar de cuenta se borran todos ({@link #clear}).
 * - La fecha de emisión permite borrar los PDF cuando el DeCA sale de «Últimos 7 Días»
 *   ({@link #prune}) sin preguntar al servidor.
 */
final class PdfStore {
    /** «Últimos 7 Días»: el servidor lista los DeCA emitidos en los últimos 7 días. */
    private static final long KEEP_MS = 7L * 24 * 3600 * 1000;
    /** Un PDF de DeCA no supera 5 MB; margen para versiones con muchas modificaciones. */
    private static final long MAX_BYTES = 20L * 1024 * 1024;

    private PdfStore() {}

    /** Qué PDF: `{id, version, creado (ISO), archivo}` (lo envía la parte web). */
    static final class Info {
        final long id;
        final int version;
        final long createdSeconds;
        final String fileName;

        private Info(long id, int version, long createdSeconds, String fileName) {
            this.id = id;
            this.version = version;
            this.createdSeconds = createdSeconds;
            this.fileName = fileName;
        }

        static Info parse(String json) throws Exception {
            JSONObject o = new JSONObject(json);
            long id = o.getLong("id");
            int version = o.optInt("version", 1);
            if (id <= 0 || version <= 0) throw new IllegalArgumentException("DeCA no válido");
            long created = Instant.parse(o.getString("creado")).getEpochSecond();
            String name = o.optString("archivo");
            boolean safe = name.endsWith(".pdf") && name.length() <= 200
                    && !name.contains("/") && !name.contains("\\") && !name.startsWith(".");
            return new Info(id, version, created, safe ? name : "DeCA-" + id + ".pdf");
        }

        String folder() {
            return id + "-v" + version + "-" + createdSeconds;
        }

        /** Ruta de la API (con sesión) que sirve este PDF. */
        String apiPath() {
            return "/api/decas/" + id + "/pdf?version=" + version;
        }
    }

    private static File root(Context c) {
        return new File(c.getFilesDir(), "pdf");
    }

    private static File file(Context c, Session.Snapshot owner, Info info) {
        return new File(new File(new File(root(c), owner.key()), info.folder()), info.fileName);
    }

    static boolean saved(Context c, Session.Snapshot owner, Info info) {
        return owner.active() && file(c, owner, info).isFile();
    }

    /** El PDF en el móvil, descargándolo antes si no estaba. */
    static File fetch(Context c, Session.Snapshot owner, Info info) throws IOException {
        File f = file(c, owner, info);
        if (f.isFile()) return f;
        File dir = f.getParentFile();
        if (dir == null || (!dir.isDirectory() && !dir.mkdirs())) throw new IOException("No se pudo guardar el PDF en el móvil");
        Http.Response r;
        try {
            r = Http.download(owner.server + info.apiPath(), owner.token, f, MAX_BYTES);
        } catch (IOException e) {
            deleteTree(dir);
            throw new IOException("Sin conexión con el servidor: no se puede descargar el PDF");
        }
        if (!r.ok()) {
            deleteTree(dir);
            String msg = "No se pudo descargar el PDF (error " + r.status + ")";
            try { msg = new JSONObject(r.body).optString("error", msg); } catch (Exception ignored) { }
            throw new IOException(msg);
        }
        return f;
    }

    /** Abre el PDF con el visor del móvil. */
    static void open(Context c, File f) throws IOException {
        Uri uri = FileProvider.getUriForFile(c, BuildConfig.APPLICATION_ID + ".pdf", f);
        Intent view = new Intent(Intent.ACTION_VIEW)
                .setDataAndType(uri, "application/pdf")
                .addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION | Intent.FLAG_ACTIVITY_NEW_TASK);
        try {
            c.startActivity(view);
        } catch (ActivityNotFoundException e) {
            throw new IOException("No hay ninguna app para ver PDF en este móvil. Instala un visor de PDF.");
        }
    }

    /** Borra los PDF de DeCA que ya no están en «Últimos 7 Días» (de todas las cuentas). */
    static void prune(Context c) {
        long limit = System.currentTimeMillis() / 1000 - KEEP_MS / 1000;
        File[] accounts = root(c).listFiles();
        if (accounts == null) return;
        for (File account : accounts) {
            File[] decas = account.listFiles();
            if (decas == null) continue;
            for (File deca : decas) {
                String name = deca.getName();
                long created;
                try {
                    created = Long.parseLong(name.substring(name.lastIndexOf('-') + 1));
                } catch (Exception e) {
                    created = 0; // nombre desconocido: se borra
                }
                if (created < limit) deleteTree(deca);
            }
            String[] left = account.list();
            if (left != null && left.length == 0) deleteTree(account);
        }
    }

    /** Borra todos los PDF descargados (cambio de cuenta o cierre de sesión). */
    static void clear(Context c) {
        deleteTree(root(c));
    }

    private static void deleteTree(File f) {
        File[] children = f.listFiles();
        if (children != null) for (File child : children) deleteTree(child);
        //noinspection ResultOfMethodCallIgnored
        f.delete();
    }
}
