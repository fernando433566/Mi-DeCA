package es.deca.app;

import android.content.Context;
import android.content.SharedPreferences;

import org.json.JSONObject;

/**
 * Servidor y token de la sesión, guardados también en Android para que la cola
 * sin conexión, las notificaciones y el widget funcionen con la app cerrada.
 */
public final class Session {
    private static final String PREFS = "deca_session";

    private Session() {}

    private static SharedPreferences prefs(Context c) {
        return c.getSharedPreferences(PREFS, Context.MODE_PRIVATE);
    }

    /** Guarda la sesión que envía la parte web ({server, token, user}). */
    public static synchronized void save(Context c, String json) {
        Snapshot previous = snapshot(c);
        try {
            JSONObject o = new JSONObject(json == null ? "{}" : json);
            prefs(c).edit()
                    .putString("server", o.optString("server", ""))
                    .putString("token", o.optString("token", ""))
                    .putString("user", o.optJSONObject("user") == null ? "" : o.optJSONObject("user").toString())
                    .apply();
        } catch (Exception e) {
            prefs(c).edit().clear().apply();
        }
        if (!previous.owner.equals(snapshot(c).owner)) {
            DecaWidget.save(c, "[]", 0);
            android.app.NotificationManager notifications = c.getSystemService(android.app.NotificationManager.class);
            if (notifications != null) notifications.cancelAll();
        }
    }

    /** Identidad inmutable para una operación asíncrona completa. */
    public static final class Snapshot {
        public final String server, token, owner;
        private Snapshot(String server, String token, String userId) {
            this.server = server;
            this.token = token;
            this.owner = server.isEmpty() || token.isEmpty() || userId.isEmpty()
                    ? "" : server + "|" + userId;
        }
        public boolean active() { return !owner.isEmpty(); }
        public String queueFile() {
            if (!active()) throw new IllegalStateException("No hay sesión activa");
            try {
                byte[] hash = java.security.MessageDigest.getInstance("SHA-256")
                        .digest(owner.getBytes(java.nio.charset.StandardCharsets.UTF_8));
                StringBuilder key = new StringBuilder("pendientes-");
                for (byte b : hash) key.append(String.format(java.util.Locale.ROOT, "%02x", b));
                return key + ".json";
            } catch (java.security.NoSuchAlgorithmException e) { throw new IllegalStateException(e); }
        }
    }

    public static synchronized Snapshot snapshot(Context c) {
        String userId = "";
        try {
            JSONObject user = new JSONObject(prefs(c).getString("user", "{}"));
            long id = user.optLong("id", 0);
            if (id > 0) userId = Long.toString(id);
        } catch (Exception ignored) { }
        return new Snapshot(server(c), token(c), userId);
    }

    public static synchronized boolean isCurrent(Context c, Snapshot snapshot) {
        Snapshot current = snapshot(c);
        return current.owner.equals(snapshot.owner) && current.token.equals(snapshot.token);
    }

    public static String server(Context c) {
        return prefs(c).getString("server", "");
    }

    public static String token(Context c) {
        return prefs(c).getString("token", "");
    }

    public static boolean active(Context c) {
        return !server(c).isEmpty() && !token(c).isEmpty();
    }
}
