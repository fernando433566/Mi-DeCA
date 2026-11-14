package es.deca.app;

import android.app.PendingIntent;
import android.content.BroadcastReceiver;
import android.content.Context;
import android.content.Intent;
import android.content.pm.PackageInstaller;
import android.util.Log;

/**
 * Resultado de instalar una actualización (ver {@link Updater}). Si Android pide que el
 * usuario lo confirme, se abre su diálogo con la app delante o, si no, se avisa con una
 * notificación. Si todo va bien, Android cierra y sustituye la app: no hay nada que hacer.
 */
public class UpdateReceiver extends BroadcastReceiver {
    @Override
    public void onReceive(Context c, Intent intent) {
        int status = intent.getIntExtra(PackageInstaller.EXTRA_STATUS, PackageInstaller.STATUS_FAILURE);
        if (status == PackageInstaller.STATUS_PENDING_USER_ACTION) {
            Intent confirm = intent.getParcelableExtra(Intent.EXTRA_INTENT);
            if (confirm == null) return;
            confirm.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK);
            if (MainActivity.foreground() != null) {
                c.startActivity(confirm);
            } else {
                Notifier.update(c, "Actualización de DeCA lista", "Toca para instalarla",
                        PendingIntent.getActivity(c, 7302, confirm, PendingIntent.FLAG_IMMUTABLE | PendingIntent.FLAG_UPDATE_CURRENT));
            }
        } else if (status != PackageInstaller.STATUS_SUCCESS) {
            String msg = intent.getStringExtra(PackageInstaller.EXTRA_STATUS_MESSAGE);
            Log.w("DeCA", "actualización no instalada: " + status + " " + msg);
            if (status == PackageInstaller.STATUS_FAILURE_CONFLICT || status == PackageInstaller.STATUS_FAILURE_INCOMPATIBLE) {
                Notifier.update(c, "No se pudo actualizar DeCA",
                        "La actualización no está firmada como la app instalada. Avisa al administrador.", null);
            }
        }
    }
}
