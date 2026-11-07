package es.deca.app;

import android.Manifest;
import android.app.NotificationChannel;
import android.app.NotificationManager;
import android.app.PendingIntent;
import android.content.Context;
import android.content.Intent;
import android.content.pm.PackageManager;
import android.graphics.Bitmap;
import android.os.Build;

import androidx.core.app.NotificationCompat;
import androidx.core.app.NotificationManagerCompat;
import androidx.core.content.ContextCompat;

import org.json.JSONObject;

/** Avisos al conductor cuando la cola sin conexión crea (o no puede crear) un DeCA. */
public final class Notifier {
    private static final String CHANNEL = "deca_cola";

    private Notifier() {}

    private static void channel(Context c) {
        NotificationManager nm = c.getSystemService(NotificationManager.class);
        NotificationChannel ch = new NotificationChannel(CHANNEL, c.getString(R.string.channel_name), NotificationManager.IMPORTANCE_HIGH);
        ch.setDescription("Avisos cuando un DeCA guardado sin conexión se crea en el servidor");
        nm.createNotificationChannel(ch);
    }

    static PendingIntent open(Context c, String route, int code) {
        Intent i = new Intent(c, MainActivity.class);
        i.putExtra(MainActivity.EXTRA_ROUTE, route);
        i.setFlags(Intent.FLAG_ACTIVITY_NEW_TASK | Intent.FLAG_ACTIVITY_CLEAR_TOP);
        return PendingIntent.getActivity(c, code, i, PendingIntent.FLAG_UPDATE_CURRENT | PendingIntent.FLAG_IMMUTABLE);
    }

    private static boolean allowed(Context c) {
        return Build.VERSION.SDK_INT < 33
                || ContextCompat.checkSelfPermission(c, Manifest.permission.POST_NOTIFICATIONS) == PackageManager.PERMISSION_GRANTED;
    }

    /** DeCA creado: notificación con el QR ya visible. */
    public static void created(Context c, JSONObject deca, int count) {
        if (!allowed(c)) return;
        channel(c);
        String numero = deca.optString("numero");
        String url = deca.optString("url");
        Bitmap qr = QrBitmap.make(url, 512);
        String text = count > 1 ? count + " DeCA creados. Ya tienes el QR." : "Ya tienes el QR. Toca para verlo.";
        NotificationCompat.Builder b = new NotificationCompat.Builder(c, CHANNEL)
                .setSmallIcon(R.drawable.ic_notification)
                .setContentTitle("DeCA " + numero + " creado")
                .setContentText(text)
                .setPriority(NotificationCompat.PRIORITY_HIGH)
                .setAutoCancel(true)
                .setContentIntent(open(c, "/deca/" + deca.optLong("id"), (int) deca.optLong("id")));
        if (qr != null) b.setLargeIcon(qr).setStyle(new NotificationCompat.BigPictureStyle().bigPicture(qr).setSummaryText(text));
        try {
            NotificationManagerCompat.from(c).notify((int) deca.optLong("id"), b.build());
        } catch (SecurityException ignored) { }
    }

    public static void failed(Context c, String msg) {
        if (!allowed(c)) return;
        channel(c);
        NotificationCompat.Builder b = new NotificationCompat.Builder(c, CHANNEL)
                .setSmallIcon(R.drawable.ic_notification)
                .setContentTitle("No se pudo crear un DeCA pendiente")
                .setContentText(msg)
                .setStyle(new NotificationCompat.BigTextStyle().bigText(msg + "\nAbre la app para revisarlo."))
                .setAutoCancel(true)
                .setContentIntent(open(c, "/", 1));
        try {
            NotificationManagerCompat.from(c).notify(2, b.build());
        } catch (SecurityException ignored) { }
    }
}
