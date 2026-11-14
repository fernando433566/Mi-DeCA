import java.util.Properties

plugins {
    id("com.android.application")
}

// Clave de firma de la versión de publicación. NO está en el repositorio: se lee de
// DeCA/secrets/keystore.properties, fuera del repositorio (o de la ruta en la variable DECA_KEYSTORE_PROPERTIES).
val keystoreProps = Properties().apply {
    val f = file(System.getenv("DECA_KEYSTORE_PROPERTIES") ?: "../../../../secrets/keystore.properties")
    if (f.exists()) f.inputStream().use { load(it) }
}
val keystoreDir = file(System.getenv("DECA_KEYSTORE_PROPERTIES") ?: "../../../../secrets/keystore.properties").parentFile

android {
    namespace = "es.deca.app"
    compileSdk = 37

    defaultConfig {
        applicationId = "es.deca.app"
        minSdk = 26
        targetSdk = 37
        versionCode = 4
        versionName = "0.2.2"
    }

    // La interfaz es la misma aplicación web que sirve el servidor (source/backend/web):
    // se incluye dentro de la app para que funcione sin conexión.
    sourceSets {
        getByName("main") {
            assets.srcDir("build/generated/webassets")
        }
    }

    signingConfigs {
        create("release") {
            if (keystoreProps.containsKey("storeFile")) {
                storeFile = File(keystoreDir, keystoreProps.getProperty("storeFile"))
                storePassword = keystoreProps.getProperty("storePassword")
                keyAlias = keystoreProps.getProperty("keyAlias")
                keyPassword = keystoreProps.getProperty("keyPassword")
            }
        }
    }

    buildTypes {
        release {
            // Sin ofuscar: el puente JavaScript (@JavascriptInterface) necesita los nombres.
            isMinifyEnabled = false
            if (keystoreProps.containsKey("storeFile")) signingConfig = signingConfigs.getByName("release")
        }
    }
    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }
    buildFeatures {
        buildConfig = true
    }
}

// Copia ../backend/web (la misma web que sirve el servidor) a assets/web antes de compilar.
val copyWeb by tasks.registering(Sync::class) {
    from(rootProject.file("../backend/web"))
    into(layout.buildDirectory.dir("generated/webassets/web"))
}
tasks.named("preBuild") { dependsOn(copyWeb) }

dependencies {
    implementation("androidx.core:core:1.19.1")
    implementation("androidx.activity:activity:1.13.0")
    implementation("androidx.webkit:webkit:1.17.1")
    implementation("androidx.work:work-runtime:2.12.0")
    implementation("com.journeyapps:zxing-android-embedded:4.3.0")
}
