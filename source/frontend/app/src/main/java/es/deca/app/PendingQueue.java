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
    private static final Object LOCK = new Object();
    /** Cola única de las versiones anteriores a la 0.2.1 (sin cuenta asociada). */
    private static final String LEGACY_FILE = "pendientes.json";

    private PendingQueue() {}

    private static File file(Context c, Session.Snapshot owner) {
        return new File(c.getFilesDir(), owner.queueFile());
    }

    public static JSONArray all(Context c) {
        return all(c, Session.snapshot(c));
    }

    static JSONArray all(Context c, Session.Snapshot owner) {
        if (!owner.active()) return new JSONArray();
        synchronized (LOCK) {
            adoptLegacy(c, owner);
            try {
                File f = file(c, owner);
                if (!f.exists()) return new JSONArray();
                return new JSONArray(new String(Files.readAllBytes(f.toPath()), StandardCharsets.UTF_8));
            } catch (Exception e) {
                return new JSONArray();
            }
        }
    }

    /**
     * Al actualizar desde una versión con la cola única, sus DeCA pendientes pasan a la
     * cuenta con la sesión abierta: es quien los creó en el móvil, porque la cola se
     * envía en cuanto hay conexión. Así no se pierde ningún DeCA tras la actualización.
     */
    private static void adoptLegacy(Context c, Session.Snapshot owner) {
        File legacy = new File(c.getFilesDir(), LEGACY_FILE);
        if (!legacy.exists()) return;
        File f = file(c, owner);
        if (!f.exists()) {
            // Caso normal: la cola antigua pasa entera a la cuenta actual.
            //noinspection ResultOfMethodCallIgnored
            legacy.renameTo(f);
            return;
        }
        try {
            JSONArray merged = new JSONArray(new String(Files.readAllBytes(f.toPath()), StandardCharsets.UTF_8));
            JSONArray old = new JSONArray(new String(Files.readAllBytes(legacy.toPath()), StandardCharsets.UTF_8));
            for (int i = 0; i < old.length(); i++) merged.put(old.get(i));
            write(c, merged, owner);
            //noinspection ResultOfMethodCallIgnored
            legacy.delete();
        } catch (Exception ignored) {
            // Se vuelve a intentar la próxima vez.
        }
    }

    private static void write(Context c, JSONArray arr, Session.Snapshot owner) {
        synchronized (LOCK) {
            try {
                File tmp = new File(c.getFilesDir(), owner.queueFile() + ".tmp");
                try (FileOutputStream out = new FileOutputStream(tmp)) {
                    out.write(arr.toString().getBytes(StandardCharsets.UTF_8));
                    out.getFD().sync();
                }
                if (!tmp.renameTo(file(c, owner))) throw new IllegalStateException("rename");
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
        Session.Snapshot owner = Session.snapshot(c);
        if (!owner.active()) throw new IllegalStateException("Inicia sesión antes de guardar un DeCA");
        synchronized (LOCK) {
            if (!payload.has("client_ref")) payload.put("client_ref", UUID.randomUUID().toString());
            JSONObject item = new JSONObject();
            item.put("id", UUID.randomUUID().toString());
            item.put("creado", now());
            item.put("estado", "pendiente");
            item.put("intentos", 0);
            item.put("payload", payload);
            JSONArray arr = all(c, owner);
            arr.put(item);
            write(c, arr, owner);
            return item;
        }
    }

    public static void update(Context c, JSONObject item, Session.Snapshot owner) {
        synchronized (LOCK) {
            JSONArray arr = all(c, owner);
            for (int i = 0; i < arr.length(); i++) {
                JSONObject o = arr.optJSONObject(i);
                if (o != null && o.optString("id").equals(item.optString("id"))) {
                    try { arr.put(i, item); } catch (Exception ignored) { }
                }
            }
            write(c, arr, owner);
        }
    }

    public static void remove(Context c, String id) {
        Session.Snapshot owner = Session.snapshot(c);
        if (!owner.active()) return;
        synchronized (LOCK) {
            JSONArray arr = all(c, owner);
            JSONArray out = new JSONArray();
            for (int i = 0; i < arr.length(); i++) {
                JSONObject o = arr.optJSONObject(i);
                if (o != null && !o.optString("id").equals(id)) out.put(o);
            }
            write(c, out, owner);
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
