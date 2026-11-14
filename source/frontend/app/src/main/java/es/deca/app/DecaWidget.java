package es.deca.app;

import android.app.PendingIntent;
import android.appwidget.AppWidgetManager;
import android.appwidget.AppWidgetProvider;
import android.content.ComponentName;
import android.content.Context;
import android.content.SharedPreferences;
import android.graphics.Bitmap;
import android.widget.RemoteViews;

import org.json.JSONArray;
import org.json.JSONObject;

import java.text.SimpleDateFormat;
import java.util.Date;
import java.util.Locale;
import java.util.TimeZone;

/**
 * Widget de Android: billete con el último DeCA (número, ruta, matrícula, fecha) y su QR.
 * El QR se genera en el móvil: funciona sin conexión.
 */
public class DecaWidget extends AppWidgetProvider {
    private static final String PREFS = "deca_widget";

    private static SharedPreferences prefs(Context c) {
        return c.getSharedPreferences(PREFS, Context.MODE_PRIVATE);
    }

    /** DeCA guardado, o null. Lee también el formato de las versiones 0.2.x (lista de viajes). */
    private static JSONObject current(Context c) {
        try {
            String deca = prefs(c).getString("deca", null);
            if (deca != null) return deca.equals("null") ? null : new JSONObject(deca);
            return new JSONArray(prefs(c).getString("viajes", "[]")).optJSONObject(0);
        } catch (Exception e) {
            return null;
        }
    }

    /** Guarda el DeCA que enseña el widget (JSON de la API) o lo vacía con null. */
    public static void save(Context c, JSONObject deca) {
        prefs(c).edit().clear().putString("deca", deca == null ? "null" : deca.toString()).apply();
        refreshAll(c);
    }

    public static void refreshAll(Context c) {
        AppWidgetManager m = AppWidgetManager.getInstance(c);
        int[] ids = m.getAppWidgetIds(new ComponentName(c, DecaWidget.class));
        if (ids.length > 0) new DecaWidget().onUpdate(c, m, ids);
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

    static RemoteViews render(Context c) {
        RemoteViews v = new RemoteViews(c.getPackageName(), R.layout.widget_deca);
        JSONObject d = current(c);
        String route = "/";
        try {
            if (d == null) throw new IllegalStateException();
            JSONObject datos = d.optJSONObject("datos");
            JSONArray envios = datos != null ? datos.optJSONArray("envios") : null;
            JSONObject env = envios != null && envios.length() > 0 ? envios.getJSONObject(0) : new JSONObject();
            v.setTextViewText(R.id.w_num, d.optString("numero"));
            v.setTextViewText(R.id.w_route, env.optString("origen") + "  →  " + env.optString("destino"));
            String meta = datos != null ? datos.optString("matricula_camion") : "";
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
        // Al tocar el billete se abre ese DeCA en la app.
        PendingIntent open = Notifier.open(c, route, 7001);
        v.setOnClickPendingIntent(R.id.w_body, open);
        v.setOnClickPendingIntent(R.id.w_stub, open);
        return v;
    }
}
