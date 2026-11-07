package es.deca.app;

import android.Manifest;
import android.content.Intent;
import android.content.pm.PackageManager;
import android.graphics.Color;
import android.net.Uri;
import android.os.Build;
import android.os.Bundle;
import android.view.WindowManager;
import android.webkit.GeolocationPermissions;
import android.webkit.ValueCallback;
import android.webkit.WebChromeClient;
import android.webkit.WebResourceRequest;
import android.webkit.WebResourceResponse;
import android.webkit.WebSettings;
import android.webkit.WebView;
import android.widget.FrameLayout;

import androidx.activity.ComponentActivity;
import androidx.activity.OnBackPressedCallback;
import androidx.activity.result.ActivityResultLauncher;
import androidx.activity.result.contract.ActivityResultContracts;
import androidx.core.content.ContextCompat;
import androidx.core.graphics.Insets;
import androidx.core.view.ViewCompat;
import androidx.core.view.WindowCompat;
import androidx.core.view.WindowInsetsCompat;
import androidx.webkit.WebViewAssetLoader;
import androidx.webkit.WebViewClientCompat;

import com.journeyapps.barcodescanner.ScanContract;
import com.journeyapps.barcodescanner.ScanOptions;

import java.lang.ref.WeakReference;
import java.util.Map;

/**
 * Pantalla principal: un WebView con la misma aplicación web que sirve el servidor
 * (incluida en la app para funcionar sin conexión) y el puente {@link DecaBridge}.
 */
public class MainActivity extends ComponentActivity {
    public static final String EXTRA_ROUTE = "route";
    /**
     * Versión mínima de Android System WebView (la interfaz y MapLibre usan funciones de
     * JavaScript de Chrome 94). En Android 8 o superior se actualiza desde Google Play.
     */
    static final int MIN_WEBVIEW = 94;
    private static final String BASE = "https://appassets.androidplatform.net/app/index.html";

    private static WeakReference<MainActivity> current = new WeakReference<>(null);

    private WebView web;
    private String scanCallbackId;
    private ValueCallback<Uri[]> fileCallback;
    private GeolocationPermissions.Callback geoCallback;
    private String geoOrigin;

    private final ActivityResultLauncher<ScanOptions> scanLauncher =
            registerForActivityResult(new ScanContract(), result -> {
                String id = scanCallbackId;
                scanCallbackId = null;
                if (id == null) return;
                if (result.getContents() == null) resolveJs(id, false, "Cancelado");
                else resolveJs(id, true, result.getContents());
            });

    private final ActivityResultLauncher<String> fileLauncher =
            registerForActivityResult(new ActivityResultContracts.GetContent(), uri -> {
                if (fileCallback != null) fileCallback.onReceiveValue(uri == null ? null : new Uri[]{uri});
                fileCallback = null;
            });

    private final ActivityResultLauncher<String[]> permLauncher =
            registerForActivityResult(new ActivityResultContracts.RequestMultiplePermissions(), (Map<String, Boolean> res) -> {
                if (geoCallback != null) {
                    boolean ok = Boolean.TRUE.equals(res.get(Manifest.permission.ACCESS_FINE_LOCATION))
                            || Boolean.TRUE.equals(res.get(Manifest.permission.ACCESS_COARSE_LOCATION));
                    geoCallback.invoke(geoOrigin, ok, false);
                    geoCallback = null;
                }
            });

    @Override
    protected void onCreate(Bundle savedInstanceState) {
        super.onCreate(savedInstanceState);
        current = new WeakReference<>(this);
        if (webViewTooOld()) {
            showWebViewUpdate();
            return;
        }
        WebView.setWebContentsDebuggingEnabled(BuildConfig.DEBUG);

        FrameLayout root = new FrameLayout(this);
        root.setBackgroundColor(Color.parseColor("#2B2D31"));
        web = new WebView(this);
        root.addView(web);
        setContentView(root);

        // Las barras del sistema no tapan el contenido (pantalla de borde a borde).
        WindowCompat.getInsetsController(getWindow(), root).setAppearanceLightStatusBars(false);
        ViewCompat.setOnApplyWindowInsetsListener(root, (v, insets) -> {
            Insets bars = insets.getInsets(WindowInsetsCompat.Type.systemBars()
                    | WindowInsetsCompat.Type.ime() | WindowInsetsCompat.Type.displayCutout());
            v.setPadding(bars.left, bars.top, bars.right, bars.bottom);
            return WindowInsetsCompat.CONSUMED;
        });

        WebSettings s = web.getSettings();
        s.setJavaScriptEnabled(true);
        s.setDomStorageEnabled(true);
        s.setGeolocationEnabled(true);
        s.setAllowFileAccess(false);
        s.setAllowContentAccess(false);
        // Permite servidores http:// en la red local (los QR públicos siempre son https).
        s.setMixedContentMode(WebSettings.MIXED_CONTENT_ALWAYS_ALLOW);
        s.setUserAgentString(s.getUserAgentString() + " DeCA-Android/" + BuildConfig.VERSION_NAME);

        WebViewAssetLoader loader = new WebViewAssetLoader.Builder()
                .addPathHandler("/app/", new WebAssets(this))
                .build();
        web.setWebViewClient(new WebViewClientCompat() {
            @Override
            public WebResourceResponse shouldInterceptRequest(WebView view, WebResourceRequest request) {
                return loader.shouldInterceptRequest(request.getUrl());
            }

            /** Si el proceso del WebView se cae (falta de memoria), se vuelve a abrir la pantalla. */
            @Override
            public boolean onRenderProcessGone(WebView view, android.webkit.RenderProcessGoneDetail detail) {
                recreate();
                return true;
            }

            @Override
            public boolean shouldOverrideUrlLoading(WebView view, WebResourceRequest request) {
                Uri u = request.getUrl();
                if ("appassets.androidplatform.net".equals(u.getHost())) return false;
                try {
                    startActivity(new Intent(Intent.ACTION_VIEW, u));
                } catch (Exception ignored) { }
                return true;
            }
        });
        web.setWebChromeClient(new WebChromeClient() {
            @Override
            public boolean onShowFileChooser(WebView view, ValueCallback<Uri[]> callback, FileChooserParams params) {
                if (fileCallback != null) fileCallback.onReceiveValue(null);
                fileCallback = callback;
                fileLauncher.launch("*/*");
                return true;
            }

            @Override
            public void onGeolocationPermissionsShowPrompt(String origin, GeolocationPermissions.Callback callback) {
                if (ContextCompat.checkSelfPermission(MainActivity.this, Manifest.permission.ACCESS_FINE_LOCATION) == PackageManager.PERMISSION_GRANTED) {
                    callback.invoke(origin, true, false);
                    return;
                }
                geoOrigin = origin;
                geoCallback = callback;
                permLauncher.launch(new String[]{Manifest.permission.ACCESS_FINE_LOCATION, Manifest.permission.ACCESS_COARSE_LOCATION});
            }
        });
        web.addJavascriptInterface(new DecaBridge(this), "DecaNative");

        getOnBackPressedDispatcher().addCallback(this, new OnBackPressedCallback(true) {
            @Override
            public void handleOnBackPressed() {
                if (web.canGoBack()) web.goBack();
                else finish();
            }
        });

        if (Build.VERSION.SDK_INT >= 33 && ContextCompat.checkSelfPermission(this, Manifest.permission.POST_NOTIFICATIONS) != PackageManager.PERMISSION_GRANTED) {
            permLauncher.launch(new String[]{Manifest.permission.POST_NOTIFICATIONS});
        }

        web.loadUrl(BASE + hash(getIntent()));
    }

    /** Versión principal del WebView instalado (p. ej. 138), o -1 si no se sabe. */
    private int webViewMajor() {
        try {
            android.content.pm.PackageInfo p = androidx.webkit.WebViewCompat.getCurrentWebViewPackage(this);
            if (p == null || p.versionName == null) return -1;
            return Integer.parseInt(p.versionName.split("\\.")[0]);
        } catch (Exception e) {
            return -1;
        }
    }

    private boolean webViewTooOld() {
        int v = webViewMajor();
        return v > 0 && v < MIN_WEBVIEW;
    }

    /** Pantalla de aviso cuando el WebView del móvil es demasiado antiguo. */
    private void showWebViewUpdate() {
        android.content.pm.PackageInfo p = androidx.webkit.WebViewCompat.getCurrentWebViewPackage(this);
        String pkg = p != null ? p.packageName : "com.google.android.webview";
        android.widget.LinearLayout box = new android.widget.LinearLayout(this);
        box.setOrientation(android.widget.LinearLayout.VERTICAL);
        box.setGravity(android.view.Gravity.CENTER);
        int pad = (int) (24 * getResources().getDisplayMetrics().density);
        box.setPadding(pad, pad, pad, pad);
        box.setBackgroundColor(Color.parseColor("#2B2D31"));
        android.widget.TextView title = new android.widget.TextView(this);
        title.setText("Hay que actualizar un componente del móvil");
        title.setTextColor(Color.WHITE);
        title.setTextSize(22);
        title.setTypeface(android.graphics.Typeface.DEFAULT_BOLD);
        android.widget.TextView text = new android.widget.TextView(this);
        text.setText("DeCA necesita «Android System WebView» versión " + MIN_WEBVIEW + " o posterior y este móvil tiene la "
                + webViewMajor() + ".\n\nPulsa el botón, actualízalo en Google Play y vuelve a abrir DeCA.");
        text.setTextColor(Color.parseColor("#DDDDDD"));
        text.setTextSize(17);
        text.setPadding(0, pad / 2, 0, pad);
        android.widget.Button btn = new android.widget.Button(this);
        btn.setText("Actualizar en Google Play");
        btn.setTextSize(18);
        btn.setOnClickListener(v -> {
            try {
                startActivity(new Intent(Intent.ACTION_VIEW, Uri.parse("market://details?id=" + pkg)));
            } catch (Exception e) {
                startActivity(new Intent(Intent.ACTION_VIEW, Uri.parse("https://play.google.com/store/apps/details?id=" + pkg)));
            }
        });
        box.addView(title);
        box.addView(text);
        box.addView(btn);
        setContentView(box);
    }

    private static String hash(Intent i) {
        String route = i == null ? null : i.getStringExtra(EXTRA_ROUTE);
        return route == null || route.isEmpty() ? "" : "#" + route;
    }

    @Override
    protected void onNewIntent(Intent intent) {
        super.onNewIntent(intent);
        if (web == null) return;
        String h = hash(intent);
        if (!h.isEmpty()) web.evaluateJavascript("location.hash=" + DecaBridge.jsString(h), null);
    }

    @Override
    protected void onResume() {
        super.onResume();
        if (web == null) {
            // Pantalla de "actualiza el WebView": si ya se ha actualizado, se abre la app.
            if (!webViewTooOld()) recreate();
            return;
        }
        if (PendingQueue.hasWork(this)) DecaSyncWorker.schedule(this, false);
        web.evaluateJavascript("window.__decaQueueChanged&&window.__decaQueueChanged()", null);
    }

    @Override
    protected void onDestroy() {
        if (current.get() == this) current = new WeakReference<>(null);
        if (web != null) web.destroy();
        super.onDestroy();
    }

    /** Avisa a la web de que la cola ha cambiado (si la app está abierta). */
    static void notifyQueueChanged() {
        MainActivity a = current.get();
        if (a != null && a.web != null) a.runOnUiThread(() -> a.web.evaluateJavascript("window.__decaQueueChanged&&window.__decaQueueChanged()", null));
    }

    void startQrScan(String callbackId) {
        scanCallbackId = callbackId;
        ScanOptions o = new ScanOptions()
                .setDesiredBarcodeFormats(ScanOptions.QR_CODE)
                .setPrompt("Apunta al código QR que te enseña el administrador")
                .setBeepEnabled(false)
                .setOrientationLocked(false);
        scanLauncher.launch(o);
    }

    void resolveJs(String id, boolean ok, String value) {
        web.evaluateJavascript("window.__decaNativeResult(" + DecaBridge.jsString(id) + "," + ok + "," + DecaBridge.jsString(value) + ")", null);
    }

    /** Brillo al máximo mientras se enseña el QR en un control. */
    void setMaxBrightness(boolean max) {
        WindowManager.LayoutParams lp = getWindow().getAttributes();
        lp.screenBrightness = max ? 1f : WindowManager.LayoutParams.BRIGHTNESS_OVERRIDE_NONE;
        getWindow().setAttributes(lp);
        if (max) getWindow().addFlags(WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON);
        else getWindow().clearFlags(WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON);
    }
}
