package es.deca.app;

import android.app.DownloadManager;
import android.content.Context;
import android.database.Cursor;
import android.net.Uri;
import android.util.Base64;

import org.json.JSONArray;
import org.json.JSONObject;

import java.io.File;
import java.io.FileInputStream;
import java.io.FileOutputStream;
import java.io.RandomAccessFile;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.security.MessageDigest;
import java.util.concurrent.ExecutorService;
import java.util.concurrent.Executors;

/**
 * Mapas por regiones guardados en el móvil (como Organic Maps): un fichero PMTiles por
 * región, descargado con el gestor de descargas de Android (reanuda si se corta) y
 * comprobado con su SHA-256. La parte web los lee por trozos con {@link #read}.
 */
public final class MapStore {
    private static final Object LOCK = new Object();
    private static final ExecutorService VERIFY = Executors.newSingleThreadExecutor();

    private MapStore() {}

    private static File dir(Context c) {
        File d = new File(c.getExternalFilesDir(null), "mapas");
        //noinspection ResultOfMethodCallIgnored
        d.mkdirs();
        return d;
    }

    private static File index(Context c) {
        return new File(dir(c), "mapas.json");
    }

    private static JSONArray load(Context c) {
        try {
            File f = index(c);
            if (!f.exists()) return new JSONArray();
            return new JSONArray(new String(Files.readAllBytes(f.toPath()), StandardCharsets.UTF_8));
        } catch (Exception e) {
            return new JSONArray();
        }
    }

    private static void store(Context c, JSONArray arr) {
        try (FileOutputStream out = new FileOutputStream(index(c))) {
            out.write(arr.toString().getBytes(StandardCharsets.UTF_8));
        } catch (Exception ignored) { }
    }

    private static JSONObject find(JSONArray arr, String id) {
        for (int i = 0; i < arr.length(); i++) {
            JSONObject o = arr.optJSONObject(i);
            if (o != null && o.optString("id").equals(id)) return o;
        }
        return null;
    }

    private static void put(JSONArray arr, JSONObject item) throws Exception {
        for (int i = 0; i < arr.length(); i++) {
            if (arr.optJSONObject(i).optString("id").equals(item.optString("id"))) {
                arr.put(i, item);
                return;
            }
        }
        arr.put(item);
    }

    private static File file(Context c, JSONObject item) {
        String id = item.optString("id"), version = item.optString("version");
        if (!id.matches("[A-Za-z0-9_-]{1,80}") || !version.matches("[0-9]{8}")) {
            throw new IllegalArgumentException("Identificador de mapa no válido");
        }
        return new File(dir(c), id + "-" + version + ".pmtiles");
    }

    /** Empieza a descargar una región (JSON del catálogo del servidor). */
    public static void download(Context c, JSONObject region) throws Exception {
        synchronized (LOCK) {
            JSONArray arr = load(c);
            JSONObject old = find(arr, region.getString("id"));
            JSONObject item = new JSONObject();
            item.put("id", region.getString("id"));
            item.put("nombre", region.optString("nombre"));
            item.put("version", region.getString("version"));
            item.put("tamano", region.optLong("tamano"));
            item.put("sha256", region.optString("sha256"));
            item.put("limites", region.optJSONObject("limites"));
            item.put("estado", "descargando");
            item.put("progreso", 0);
            if (old != null && old.optString("estado").equals("descargado")) item.put("anterior", old.optString("version"));
            File dest = file(c, item);
            //noinspection ResultOfMethodCallIgnored
            dest.delete();
            Session.Snapshot owner = Session.snapshot(c);
            if (!owner.active()) throw new IllegalStateException("Inicia sesión para descargar mapas");
            String destination = ServerUrl.authenticated(owner.server, owner.server + region.getString("url"));
            DownloadManager.Request req = new DownloadManager.Request(Uri.parse(destination))
                    .addRequestHeader("Authorization", "Bearer " + owner.token)
                    .setTitle("Mapa de " + region.optString("nombre"))
                    .setDescription("DeCA · mapa sin conexión")
                    .setNotificationVisibility(DownloadManager.Request.VISIBILITY_VISIBLE)
                    .setDestinationUri(Uri.fromFile(dest));
            DownloadManager dm = c.getSystemService(DownloadManager.class);
            item.put("descarga", dm.enqueue(req));
            put(arr, item);
            store(c, arr);
        }
    }

    /** Estado de las regiones del móvil (actualiza el progreso de las descargas). */
    public static JSONArray list(Context c) {
        synchronized (LOCK) {
            JSONArray arr = load(c);
            DownloadManager dm = c.getSystemService(DownloadManager.class);
            boolean changed = false;
            for (int i = 0; i < arr.length(); i++) {
                JSONObject it = arr.optJSONObject(i);
                if (!it.optString("estado").equals("descargando")) continue;
                try (Cursor cur = dm.query(new DownloadManager.Query().setFilterById(it.optLong("descarga")))) {
                    if (cur == null || !cur.moveToFirst()) {
                        it.put("estado", "error");
                        changed = true;
                        continue;
                    }
                    int status = cur.getInt(cur.getColumnIndexOrThrow(DownloadManager.COLUMN_STATUS));
                    long done = cur.getLong(cur.getColumnIndexOrThrow(DownloadManager.COLUMN_BYTES_DOWNLOADED_SO_FAR));
                    long total = cur.getLong(cur.getColumnIndexOrThrow(DownloadManager.COLUMN_TOTAL_SIZE_BYTES));
                    if (total <= 0) total = it.optLong("tamano", 1);
                    it.put("progreso", Math.min(1.0, done / (double) Math.max(1, total)));
                    if (status == DownloadManager.STATUS_SUCCESSFUL) {
                        it.put("estado", "verificando");
                        verifyLater(c, it.optString("id"));
                    } else if (status == DownloadManager.STATUS_FAILED) {
                        it.put("estado", "error");
                    }
                    changed = true;
                } catch (Exception ignored) { }
            }
            if (changed) store(c, arr);
            // La web trata "verificando" como si siguiera descargando.
            JSONArray out = new JSONArray();
            for (int i = 0; i < arr.length(); i++) {
                JSONObject o = arr.optJSONObject(i);
                try {
                    JSONObject copy = new JSONObject(o.toString());
                    if (o.optString("estado").equals("verificando")) copy.put("estado", "descargando");
                    out.put(copy);
                } catch (Exception ignored) { }
            }
            return out;
        }
    }

    private static void verifyLater(Context c, String id) {
        VERIFY.execute(() -> {
            JSONObject it;
            synchronized (LOCK) {
                it = find(load(c), id);
            }
            if (it == null) return;
            File f = file(c, it);
            boolean ok = f.exists() && sha256(f).equalsIgnoreCase(it.optString("sha256"));
            synchronized (LOCK) {
                JSONArray arr = load(c);
                JSONObject cur = find(arr, id);
                if (cur == null) return;
                try {
                    cur.put("estado", ok ? "descargado" : "error");
                    cur.put("progreso", 1);
                    if (ok && cur.has("anterior") && !cur.optString("anterior").equals(cur.optString("version"))) {
                        //noinspection ResultOfMethodCallIgnored
                        new File(dir(c), id + "-" + cur.optString("anterior") + ".pmtiles").delete();
                    }
                    cur.remove("anterior");
                    if (!ok) {
                        //noinspection ResultOfMethodCallIgnored
                        f.delete();
                    }
                    store(c, arr);
                } catch (Exception ignored) { }
            }
        });
    }

    private static String sha256(File f) {
        try (FileInputStream in = new FileInputStream(f)) {
            MessageDigest md = MessageDigest.getInstance("SHA-256");
            byte[] buf = new byte[1 << 16];
            int n;
            while ((n = in.read(buf)) > 0) md.update(buf, 0, n);
            StringBuilder sb = new StringBuilder();
            for (byte b : md.digest()) sb.append(String.format(java.util.Locale.ROOT, "%02x", b));
            return sb.toString();
        } catch (Exception e) {
            return "";
        }
    }

    public static void delete(Context c, String id) {
        synchronized (LOCK) {
            JSONArray arr = load(c);
            JSONArray out = new JSONArray();
            for (int i = 0; i < arr.length(); i++) {
                JSONObject o = arr.optJSONObject(i);
                if (o.optString("id").equals(id)) {
                    if (o.has("descarga")) c.getSystemService(DownloadManager.class).remove(o.optLong("descarga"));
                    //noinspection ResultOfMethodCallIgnored
                    file(c, o).delete();
                } else {
                    out.put(o);
                }
            }
            store(c, out);
        }
    }

    /** Lee `length` bytes desde `offset` de un mapa descargado (Base64). */
    public static String read(Context c, String id, long offset, int length) {
        if (offset < 0 || length < 0 || length > 4 * 1024 * 1024) return null;
        JSONObject it;
        synchronized (LOCK) {
            it = find(load(c), id);
        }
        if (it == null) return null;
        try (RandomAccessFile f = new RandomAccessFile(file(c, it), "r")) {
            int len = (int) Math.max(0, Math.min(length, f.length() - offset));
            byte[] buf = new byte[len];
            f.seek(offset);
            f.readFully(buf);
            return Base64.encodeToString(buf, Base64.NO_WRAP);
        } catch (Exception e) {
            return null;
        }
    }
}
