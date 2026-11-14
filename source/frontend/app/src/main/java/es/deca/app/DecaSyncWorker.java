package es.deca.app;

import android.content.Context;

import androidx.annotation.NonNull;
import androidx.work.BackoffPolicy;
import androidx.work.Constraints;
import androidx.work.ExistingWorkPolicy;
import androidx.work.NetworkType;
import androidx.work.OneTimeWorkRequest;
import androidx.work.WorkManager;
import androidx.work.Worker;
import androidx.work.WorkerParameters;

import org.json.JSONArray;
import org.json.JSONObject;

import java.io.IOException;
import java.util.concurrent.TimeUnit;

/**
 * Envía al servidor los DeCA de la cola en cuanto hay conexión.
 * Si el servidor no responde, WorkManager lo reintenta más tarde (espera creciente).
 */
public class DecaSyncWorker extends Worker {
    private static final String WORK = "deca-sync";

    public DecaSyncWorker(@NonNull Context context, @NonNull WorkerParameters params) {
        super(context, params);
    }

    /** Programa el envío (se ejecuta cuando haya red). */
    public static void schedule(Context c, boolean replace) {
        OneTimeWorkRequest req = new OneTimeWorkRequest.Builder(DecaSyncWorker.class)
                .setConstraints(new Constraints.Builder().setRequiredNetworkType(NetworkType.CONNECTED).build())
                .setBackoffCriteria(BackoffPolicy.EXPONENTIAL, 15, TimeUnit.SECONDS)
                .build();
        WorkManager.getInstance(c).enqueueUniqueWork(WORK,
                replace ? ExistingWorkPolicy.REPLACE : ExistingWorkPolicy.KEEP, req);
    }

    @NonNull
    @Override
    public Result doWork() {
        Context c = getApplicationContext();
        Session.Snapshot owner = Session.snapshot(c);
        if (!owner.active()) return Result.success();
        String server = owner.server;
        String token = owner.token;
        boolean retry = false;
        JSONArray items = PendingQueue.all(c, owner);
        for (int i = 0; i < items.length(); i++) {
            if (!Session.isCurrent(c, owner)) break;
            JSONObject item = items.optJSONObject(i);
            String estado = item.optString("estado");
            if (!estado.equals("pendiente") && !estado.equals("enviando")) continue;
            try {
                item.put("estado", "enviando");
                item.put("intentos", item.optInt("intentos") + 1);
                PendingQueue.update(c, item, owner);
                Http.Response r = Http.request("POST", server + "/api/decas", token, item.getJSONObject("payload").toString());
                if (r.ok()) {
                    JSONObject res = new JSONObject(r.body);
                    item.put("estado", "creado");
                    item.put("resultado", res);
                    PendingQueue.update(c, item, owner);
                    JSONArray decas = res.optJSONArray("decas");
                    synchronized (Session.class) {
                    if (Session.isCurrent(c, owner) && decas != null && decas.length() > 0) {
                        DecaWidget.save(c, decas.getJSONObject(0));
                        Notifier.created(c, decas.getJSONObject(0));
                    }
                    }
                } else if (r.status >= 500 || r.status == 408 || r.status == 429) {
                    item.put("estado", "pendiente");
                    PendingQueue.update(c, item, owner);
                    retry = true;
                } else {
                    String msg = "Error " + r.status;
                    try { msg = new JSONObject(r.body).optString("error", msg); } catch (Exception ignored) { }
                    item.put("estado", "error");
                    item.put("error", msg);
                    PendingQueue.update(c, item, owner);
                    synchronized (Session.class) {
                        if (Session.isCurrent(c, owner)) Notifier.failed(c, msg);
                    }
                }
            } catch (IOException e) {
                // Sin conexión con el servidor: se vuelve a intentar.
                try { item.put("estado", "pendiente"); } catch (Exception ignored) { }
                PendingQueue.update(c, item, owner);
                retry = true;
            } catch (Exception e) {
                try {
                    item.put("estado", "error");
                    item.put("error", "Respuesta no válida del servidor");
                } catch (Exception ignored) { }
                PendingQueue.update(c, item, owner);
            }
        }
        MainActivity.notifyQueueChanged();
        return retry ? Result.retry() : Result.success();
    }
}
