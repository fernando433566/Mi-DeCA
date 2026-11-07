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
    public static void save(Context c, String json) {
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
