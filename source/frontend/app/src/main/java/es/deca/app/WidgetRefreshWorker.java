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
        if (!Session.active(c)) return Result.success();
        try {
            Http.Response r = Http.request("GET", Session.server(c) + "/api/decas/ultimo", Session.token(c), null);
            if (r.ok()) {
                // Viajes del último DeCA; si es el mismo, se mantiene el viaje que se mostraba.
                DecaWidget.saveKeepingIndex(c, new JSONArray(r.body.trim()).toString());
            }
        } catch (Exception ignored) {
            // Sin conexión: se mantiene lo último guardado.
        }
        return Result.success();
    }
}
