package es.deca.app;

import android.content.Context;

import androidx.annotation.NonNull;
import androidx.work.Constraints;
import androidx.work.ExistingPeriodicWorkPolicy;
import androidx.work.ExistingWorkPolicy;
import androidx.work.NetworkType;
import androidx.work.OneTimeWorkRequest;
import androidx.work.PeriodicWorkRequest;
import androidx.work.WorkManager;
import androidx.work.Worker;
import androidx.work.WorkerParameters;

import java.util.concurrent.TimeUnit;

/** Búsqueda automática de actualizaciones: cada 12 h y al abrir la app (ver {@link Updater}). */
public class UpdateWorker extends Worker {
    private static final long OPEN_INTERVAL_MS = 6L * 3600 * 1000;

    public UpdateWorker(@NonNull Context c, @NonNull WorkerParameters p) {
        super(c, p);
    }

    private static Constraints network() {
        return new Constraints.Builder().setRequiredNetworkType(NetworkType.CONNECTED).build();
    }

    /** Programa la comprobación periódica (si ya estaba programada, no la duplica). */
    static void schedule(Context c) {
        PeriodicWorkRequest req = new PeriodicWorkRequest.Builder(UpdateWorker.class, 12, TimeUnit.HOURS)
                .setConstraints(network()).build();
        WorkManager.getInstance(c).enqueueUniquePeriodicWork("deca-update", ExistingPeriodicWorkPolicy.KEEP, req);
    }

    /** Al abrir la app: comprueba ahora si hace más de 6 h de la última vez. */
    static void checkOnOpen(Context c) {
        if (Updater.sinceLastCheck(c) < OPEN_INTERVAL_MS) return;
        OneTimeWorkRequest req = new OneTimeWorkRequest.Builder(UpdateWorker.class).setConstraints(network()).build();
        WorkManager.getInstance(c).enqueueUniqueWork("deca-update-open", ExistingWorkPolicy.KEEP, req);
    }

    @NonNull
    @Override
    public Result doWork() {
        try {
            Updater.checkAndInstall(getApplicationContext(), false);
        } catch (Exception ignored) {
            // Sin conexión, sin versión publicada… se vuelve a intentar en la próxima comprobación.
        }
        return Result.success();
    }
}
