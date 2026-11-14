package es.deca.app;

import android.app.PendingIntent;
import android.content.Context;
import android.content.Intent;
import android.content.SharedPreferences;
import android.content.pm.PackageInstaller;
import android.net.Uri;
import android.os.Build;
import android.provider.Settings;

import org.json.JSONObject;

import java.io.File;
import java.io.FileInputStream;
import java.io.IOException;
import java.io.InputStream;
import java.io.OutputStream;
import java.security.MessageDigest;
import java.util.Locale;

/**
 * Actualizaciones de la app: pregunta al servidor DeCA por la última versión (el administrador
 * elige el origen en Ajustes → Servidor), descarga la APK y la instala con el instalador de
 * paquetes de Android.
 *
 * - Automático ({@link UpdateWorker}): al abrir la app y cada 12 h, si el administrador lo tiene activado.
 * - Manual: Ajustes → «Versión de la app» → Buscar actualizaciones.
 *
 * Android solo acepta la APK si está firmada con la misma clave que la app instalada. Si puede,
 * la instala sin preguntar; si no, pide confirmación (ver {@link UpdateReceiver}).
 */
final class Updater {
    /** Resultado de una comprobación. */
    enum Result { DISABLED, UP_TO_DATE, INSTALLING }

    private static final String PREFS = "deca_updates";
    /** Una APK de DeCA ocupa unos 5 MB. */
    private static final long MAX_APK = 100L * 1024 * 1024;

    private Updater() {}

    private static SharedPreferences prefs(Context c) {
        return c.getSharedPreferences(PREFS, Context.MODE_PRIVATE);
    }

    /** Milisegundos desde la última comprobación automática. */
    static long sinceLastCheck(Context c) {
        return System.currentTimeMillis() - prefs(c).getLong("ultima", 0);
    }

    /**
     * Comprueba y, si hay versión nueva, la descarga e instala.
     * @param manual  pulsado por el usuario: se comprueba aunque el modo automático esté desactivado
     * @throws IOException con un mensaje para el usuario si algo falla
     */
    static synchronized Result checkAndInstall(Context c, boolean manual) throws IOException {
        Session.Snapshot owner = Session.snapshot(c);
        if (!owner.active()) throw new IOException("Inicia sesión para buscar actualizaciones");
        Http.Response r = Http.request("GET", owner.server + "/api/app/update", owner.token, null);
        if (!r.ok()) throw new IOException("El servidor respondió con el error " + r.status);
        JSONObject info;
        try {
            info = new JSONObject(r.body);
        } catch (Exception e) {
            throw new IOException("Respuesta no válida del servidor");
        }
        if (!manual) prefs(c).edit().putLong("ultima", System.currentTimeMillis()).apply();
        if (!manual && !info.optBoolean("automatico", false)) return Result.DISABLED;
        JSONObject latest = info.optJSONObject("ultima");
        if (latest == null) {
            throw new IOException(info.optString("error", "No se ha encontrado ninguna versión publicada"));
        }
        String version = latest.optString("version");
        if (!isNewer(version, BuildConfig.VERSION_NAME)) return Result.UP_TO_DATE;
        if (!c.getPackageManager().canRequestPackageInstalls()) {
            askInstallPermission(c, manual);
            throw new IOException("Permite que DeCA instale aplicaciones y vuelve a intentarlo");
        }
        File apk = download(c, latest.optString("apk"), latest.optString("sha256", ""), version);
        install(c, apk);
        return Result.INSTALLING;
    }

    /** ¿`candidate` es posterior a `current`? Compara por números: 0.10.0 > 0.9.2. */
    static boolean isNewer(String candidate, String current) {
        String[] a = candidate.split("[^0-9]+"), b = current.split("[^0-9]+");
        for (int i = 0; i < Math.max(a.length, b.length); i++) {
            long x = i < a.length && !a[i].isEmpty() ? Long.parseLong(a[i]) : 0;
            long y = i < b.length && !b[i].isEmpty() ? Long.parseLong(b[i]) : 0;
            if (x != y) return x > y;
        }
        return false;
    }

    /** Carpeta de la APK descargada; se vacía al abrir la app ({@link #cleanup}). */
    private static File dir(Context c) {
        return new File(c.getCacheDir(), "updates");
    }

    static void cleanup(Context c) {
        File[] files = dir(c).listFiles();
        if (files != null) for (File f : files) //noinspection ResultOfMethodCallIgnored
            f.delete();
    }

    /** La APK viene de fuera del servidor DeCA: se descarga sin el token de la sesión. */
    private static File download(Context c, String url, String sha256, String version) throws IOException {
        if (!url.startsWith("https://")) throw new IOException("La actualización debe descargarse por https://");
        File d = dir(c);
        if (!d.isDirectory() && !d.mkdirs()) throw new IOException("No se pudo guardar la actualización");
        File apk = new File(d, "DeCA-" + version.replaceAll("[^0-9A-Za-z.-]", "_") + ".apk");
        Http.Response r;
        try {
            r = Http.download(url, null, apk, MAX_APK);
        } catch (IOException e) {
            throw new IOException("No se pudo descargar la actualización");
        }
        if (!r.ok()) throw new IOException("No se pudo descargar la actualización (error " + r.status + ")");
        if (!sha256.isEmpty() && !sha256.equalsIgnoreCase(sha256(apk))) {
            //noinspection ResultOfMethodCallIgnored
            apk.delete();
            throw new IOException("La actualización descargada está dañada (SHA-256 distinto)");
        }
        return apk;
    }

    private static String sha256(File f) throws IOException {
        try (InputStream in = new FileInputStream(f)) {
            MessageDigest md = MessageDigest.getInstance("SHA-256");
            byte[] buf = new byte[65536];
            int n;
            while ((n = in.read(buf)) > 0) md.update(buf, 0, n);
            StringBuilder hex = new StringBuilder();
            for (byte b : md.digest()) hex.append(String.format(Locale.ROOT, "%02x", b));
            return hex.toString();
        } catch (java.security.NoSuchAlgorithmException e) {
            throw new IOException(e);
        }
    }

    /** Entrega la APK al instalador de Android; el resultado llega a {@link UpdateReceiver}. */
    private static void install(Context c, File apk) throws IOException {
        PackageInstaller installer = c.getPackageManager().getPackageInstaller();
        PackageInstaller.SessionParams params = new PackageInstaller.SessionParams(PackageInstaller.SessionParams.MODE_FULL_INSTALL);
        params.setAppPackageName(c.getPackageName());
        if (Build.VERSION.SDK_INT >= 31) {
            // Sin preguntar cuando Android lo permite (DeCA ya es quien instaló la app).
            params.setRequireUserAction(PackageInstaller.SessionParams.USER_ACTION_NOT_REQUIRED);
        }
        int id = installer.createSession(params);
        try (PackageInstaller.Session session = installer.openSession(id)) {
            try (InputStream in = new FileInputStream(apk); OutputStream out = session.openWrite("base.apk", 0, apk.length())) {
                byte[] buf = new byte[65536];
                int n;
                while ((n = in.read(buf)) > 0) out.write(buf, 0, n);
                session.fsync(out);
            }
            Intent result = new Intent(c, UpdateReceiver.class).setPackage(c.getPackageName());
            // Mutable: el sistema añade el estado de la instalación a este Intent.
            int flags = PendingIntent.FLAG_UPDATE_CURRENT | (Build.VERSION.SDK_INT >= 31 ? PendingIntent.FLAG_MUTABLE : 0);
            session.commit(PendingIntent.getBroadcast(c, id, result, flags).getIntentSender());
        } catch (IOException | RuntimeException e) {
            installer.abandonSession(id);
            throw new IOException("No se pudo instalar la actualización");
        }
    }

    /** Pantalla de Android «Instalar aplicaciones desconocidas» para DeCA. */
    private static void askInstallPermission(Context c, boolean manual) {
        Intent settings = new Intent(Settings.ACTION_MANAGE_UNKNOWN_APP_SOURCES, Uri.parse("package:" + c.getPackageName()))
                .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK);
        if (manual || MainActivity.foreground() != null) {
            c.startActivity(settings);
        } else {
            Notifier.update(c, "Actualización de DeCA disponible",
                    "Toca para permitir que DeCA instale sus actualizaciones",
                    PendingIntent.getActivity(c, 7301, settings, PendingIntent.FLAG_IMMUTABLE));
        }
    }
}
