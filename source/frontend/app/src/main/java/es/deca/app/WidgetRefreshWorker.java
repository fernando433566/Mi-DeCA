package es.deca.app;

import android.content.Context;

import androidx.annotation.NonNull;
import androidx.work.Constraints;
import androidx.work.ExistingPeriodicWorkPolicy;
import androidx.work.NetworkType;
import androidx.work.PeriodicWorkRequest;
import androidx.work.WorkManager;
import androidx.work.Worker;
import androidx.work.WorkerParameters;

import org.json.JSONArray;

import java.util.concurrent.TimeUnit;

/** Cada 30 minutos (con conexión) trae el último DeCA para el widget. */
public class WidgetRefreshWorker extends Worker {
    public WidgetRefreshWorker(@NonNull Context c, @NonNull WorkerParameters p) {
        super(c, p);
    }

    public static void schedule(Context c) {
        PeriodicWorkRequest req = new PeriodicWorkRequest.Builder(WidgetRefreshWorker.class, 30, TimeUnit.MINUTES)
                .setConstraints(new Constraints.Builder().setRequiredNetworkType(NetworkType.CONNECTED).build())
                .build();
        WorkManager.getInstance(c).enqueueUniquePeriodicWork("deca-widget", ExistingPeriodicWorkPolicy.KEEP, req);
    }

    @NonNull
    @Override
    public Result doWork() {
        Context c = getApplicationContext();
        // Cada 30 min: borra los PDF de DeCA que ya no están en «Últimos 7 Días».
        PdfStore.prune(c);
        Session.Snapshot owner = Session.snapshot(c);
        if (!owner.active()) return Result.success();
        try {
            Http.Response r = Http.request("GET", owner.server + "/api/decas/ultimo", owner.token, null);
            if (r.ok()) {
                // El servidor devuelve una lista con 0 o 1 DeCA.
                synchronized (Session.class) {
                    if (Session.isCurrent(c, owner)) DecaWidget.save(c, new JSONArray(r.body.trim()).optJSONObject(0));
                }
            }
        } catch (Exception ignored) {
            // Sin conexión: se mantiene lo último guardado.
        }
        return Result.success();
    }
}
