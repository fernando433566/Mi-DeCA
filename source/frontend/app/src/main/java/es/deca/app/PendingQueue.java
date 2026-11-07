package es.deca.app;

import android.content.Context;

import org.json.JSONArray;
import org.json.JSONObject;

import java.io.File;
import java.io.FileOutputStream;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.text.SimpleDateFormat;
import java.util.Date;
import java.util.Locale;
import java.util.TimeZone;
import java.util.UUID;

/**
 * Cola de DeCA creados sin conexión. Se guarda en un fichero JSON privado de la app.
 * Estados: pendiente -> enviando -> creado | error.
 * Cada petición lleva un `client_ref` único para que el servidor no la duplique si se reintenta.
 */
public final class PendingQueue {
    private static final String FILE = "pendientes.json";
    private static final Object LOCK = new Object();

    private PendingQueue() {}

    private static File file(Context c) {
        return new File(c.getFilesDir(), FILE);
    }

    public static JSONArray all(Context c) {
        synchronized (LOCK) {
            try {
                File f = file(c);
                if (!f.exists()) return new JSONArray();
                return new JSONArray(new String(Files.readAllBytes(f.toPath()), StandardCharsets.UTF_8));
            } catch (Exception e) {
                return new JSONArray();
            }
        }
    }

    private static void write(Context c, JSONArray arr) {
        synchronized (LOCK) {
            try {
                File tmp = new File(c.getFilesDir(), FILE + ".tmp");
                try (FileOutputStream out = new FileOutputStream(tmp)) {
                    out.write(arr.toString().getBytes(StandardCharsets.UTF_8));
                    out.getFD().sync();
                }
                if (!tmp.renameTo(file(c))) throw new IllegalStateException("rename");
            } catch (Exception ignored) {
                // Sin espacio: no se puede hacer nada más.
            }
        }
    }

    static String now() {
        SimpleDateFormat f = new SimpleDateFormat("yyyy-MM-dd'T'HH:mm:ss'Z'", Locale.ROOT);
        f.setTimeZone(TimeZone.getTimeZone("UTC"));
        return f.format(new Date());
    }

    public static JSONObject add(Context c, JSONObject payload) throws Exception {
        synchronized (LOCK) {
            if (!payload.has("client_ref")) payload.put("client_ref", UUID.randomUUID().toString());
            JSONObject item = new JSONObject();
            item.put("id", UUID.randomUUID().toString());
            item.put("creado", now());
            item.put("estado", "pendiente");
            item.put("intentos", 0);
            item.put("payload", payload);
            JSONArray arr = all(c);
            arr.put(item);
            write(c, arr);
            return item;
        }
    }

    public static void update(Context c, JSONObject item) {
        synchronized (LOCK) {
            JSONArray arr = all(c);
            for (int i = 0; i < arr.length(); i++) {
                JSONObject o = arr.optJSONObject(i);
                if (o != null && o.optString("id").equals(item.optString("id"))) {
                    try { arr.put(i, item); } catch (Exception ignored) { }
                }
            }
            write(c, arr);
        }
    }

    public static void remove(Context c, String id) {
        synchronized (LOCK) {
            JSONArray arr = all(c);
            JSONArray out = new JSONArray();
            for (int i = 0; i < arr.length(); i++) {
                JSONObject o = arr.optJSONObject(i);
                if (o != null && !o.optString("id").equals(id)) out.put(o);
            }
            write(c, out);
        }
    }

    public static boolean hasWork(Context c) {
        JSONArray arr = all(c);
        for (int i = 0; i < arr.length(); i++) {
            String s = arr.optJSONObject(i).optString("estado");
            if (s.equals("pendiente") || s.equals("enviando")) return true;
        }
        return false;
    }
}
