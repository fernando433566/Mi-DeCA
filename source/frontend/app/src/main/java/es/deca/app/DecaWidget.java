package es.deca.app;

import android.app.PendingIntent;
import android.appwidget.AppWidgetManager;
import android.appwidget.AppWidgetProvider;
import android.content.ComponentName;
import android.content.Context;
import android.content.Intent;
import android.content.SharedPreferences;
import android.graphics.Bitmap;
import android.view.View;
import android.widget.RemoteViews;

import org.json.JSONArray;
import org.json.JSONObject;

import java.text.SimpleDateFormat;
import java.util.Date;
import java.util.Locale;
import java.util.TimeZone;

/**
 * Widget de Android: billete con el último DeCA (número, ruta, matrícula, fecha) y su QR.
 * Si el DeCA tiene varios viajes (ida y vuelta) se empieza por el viaje 1 y las flechas
 * izquierda / derecha pasan al viaje anterior / siguiente. El viaje elegido se comparte
 * con la app (pantalla de inicio). El QR se genera en el móvil: funciona sin conexión.
 */
public class DecaWidget extends AppWidgetProvider {
    private static final String PREFS = "deca_widget";
    private static final String ACTION_PREV = "es.deca.app.WIDGET_PREV";
    private static final String ACTION_NEXT = "es.deca.app.WIDGET_NEXT";

    private static SharedPreferences prefs(Context c) {
        return c.getSharedPreferences(PREFS, Context.MODE_PRIVATE);
    }

    private static JSONArray trips(Context c) {
        try {
            return new JSONArray(prefs(c).getString("viajes", "[]"));
        } catch (Exception e) {
            return new JSONArray();
        }
    }

    private static String group(JSONArray trips) {
        JSONObject first = trips.optJSONObject(0);
        return first == null ? "" : first.optString("grupo_id");
    }

    /** Guarda los viajes del último DeCA (lista del servidor, del 1 al último) y el que se muestra. */
    public static void save(Context c, String tripsJson, int index) {
        prefs(c).edit().putString("viajes", tripsJson == null ? "[]" : tripsJson).putInt("indice", Math.max(0, index)).apply();
        refreshAll(c);
    }

    /** Igual que {@link #save}, pero si es el mismo DeCA mantiene el viaje que se estaba mostrando. */
    public static void saveKeepingIndex(Context c, String tripsJson) {
        int index = 0;
        try {
            if (group(new JSONArray(tripsJson)).equals(group(trips(c)))) index = prefs(c).getInt("indice", 0);
        } catch (Exception ignored) { }
        save(c, tripsJson, index);
    }

    /** Viaje que se muestra ahora: {"grupo_id": "...", "indice": n}. */
    public static String state(Context c) {
        try {
            JSONObject o = new JSONObject();
            o.put("grupo_id", group(trips(c)));
            o.put("indice", prefs(c).getInt("indice", 0));
            return o.toString();
        } catch (Exception e) {
            return "null";
        }
    }

    public static void refreshAll(Context c) {
        AppWidgetManager m = AppWidgetManager.getInstance(c);
        int[] ids = m.getAppWidgetIds(new ComponentName(c, DecaWidget.class));
        if (ids.length > 0) new DecaWidget().onUpdate(c, m, ids);
    }

    @Override
    public void onReceive(Context c, Intent intent) {
        String action = intent.getAction();
        if (ACTION_PREV.equals(action) || ACTION_NEXT.equals(action)) {
            int n = trips(c).length();
            int i = prefs(c).getInt("indice", 0) + (ACTION_NEXT.equals(action) ? 1 : -1);
            prefs(c).edit().putInt("indice", Math.max(0, Math.min(n - 1, i))).apply();
            refreshAll(c);
            return;
        }
        super.onReceive(c, intent);
    }

    @Override
    public void onUpdate(Context c, AppWidgetManager m, int[] ids) {
        RemoteViews v = render(c);
        for (int id : ids) m.updateAppWidget(id, v);
        WidgetRefreshWorker.schedule(c);
    }

    private static String fecha(String iso) {
        if (iso == null || iso.length() < 10) return "";
        try {
            if (iso.length() == 10) return iso.substring(8, 10) + "/" + iso.substring(5, 7) + "/" + iso.substring(0, 4);
            SimpleDateFormat in = new SimpleDateFormat("yyyy-MM-dd'T'HH:mm:ss", Locale.ROOT);
            in.setTimeZone(TimeZone.getTimeZone("UTC"));
            Date d = in.parse(iso.substring(0, 19));
            SimpleDateFormat out = new SimpleDateFormat("dd/MM/yyyy", Locale.ROOT);
            out.setTimeZone(TimeZone.getTimeZone("Europe/Madrid"));
            return out.format(d);
        } catch (Exception e) {
            return iso.substring(0, 10);
        }
    }

    private static PendingIntent arrow(Context c, String action, int code) {
        Intent i = new Intent(c, DecaWidget.class).setAction(action);
        return PendingIntent.getBroadcast(c, code, i, PendingIntent.FLAG_UPDATE_CURRENT | PendingIntent.FLAG_IMMUTABLE);
    }

    static RemoteViews render(Context c) {
        RemoteViews v = new RemoteViews(c.getPackageName(), R.layout.widget_deca);
        JSONArray trips = trips(c);
        int n = trips.length();
        int index = Math.max(0, Math.min(n - 1, prefs(c).getInt("indice", 0)));
        String route = "/";
        try {
            if (n == 0) throw new IllegalStateException();
            JSONObject d = trips.getJSONObject(index);
            JSONObject datos = d.optJSONObject("datos");
            JSONArray envios = datos != null ? datos.optJSONArray("envios") : null;
            JSONObject env = envios != null && envios.length() > 0 ? envios.getJSONObject(0) : new JSONObject();
            v.setTextViewText(R.id.w_num, d.optString("numero"));
            v.setTextViewText(R.id.w_route, env.optString("origen") + "  →  " + env.optString("destino"));
            String meta = datos != null ? datos.optString("matricula_camion") : "";
            if (n > 1) meta += "  ·  Viaje " + (index + 1) + "/" + n;
            if (!d.optBoolean("url_activa", true)) meta += "  ·  Enlace caducado";
            v.setTextViewText(R.id.w_meta, meta);
            v.setTextViewText(R.id.w_date, fecha(d.optString("creado")));
            Bitmap qr = QrBitmap.make(d.optString("url"), 360);
            if (qr != null) v.setImageViewBitmap(R.id.w_qr, qr);
            route = "/deca/" + d.optLong("id");
        } catch (Exception e) {
            v.setTextViewText(R.id.w_num, "Sin DeCA todavía");
            v.setTextViewText(R.id.w_route, Session.active(c) ? "Abre la app para crear uno" : "Abre la app e inicia sesión");
            v.setTextViewText(R.id.w_meta, "");
            v.setTextViewText(R.id.w_date, "");
            v.setImageViewResource(R.id.w_qr, R.drawable.ic_notification);
        }
        // Flechas solo con varios viajes; ocultas (sin mover el billete) en los extremos.
        if (n > 1) {
            v.setViewVisibility(R.id.w_prev, index > 0 ? View.VISIBLE : View.INVISIBLE);
            v.setViewVisibility(R.id.w_next, index < n - 1 ? View.VISIBLE : View.INVISIBLE);
            v.setOnClickPendingIntent(R.id.w_prev, arrow(c, ACTION_PREV, 7101));
            v.setOnClickPendingIntent(R.id.w_next, arrow(c, ACTION_NEXT, 7102));
        } else {
            v.setViewVisibility(R.id.w_prev, View.GONE);
            v.setViewVisibility(R.id.w_next, View.GONE);
        }
        // Al tocar el billete se abre ese viaje en la app.
        PendingIntent open = Notifier.open(c, route, 7001);
        v.setOnClickPendingIntent(R.id.w_body, open);
        v.setOnClickPendingIntent(R.id.w_stub, open);
        return v;
    }
}
