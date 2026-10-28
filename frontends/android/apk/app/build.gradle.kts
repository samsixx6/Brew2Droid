plugins {
    id("com.android.application")
}

// Biblioteca e ajustes usam Views; a execução do jogo continua no .so.
android {
    namespace = "io.github.brew.handset"
    compileSdk { version = release(37) }

    defaultConfig {
        applicationId = "io.github.brew.handset"
        // O AAudio, que é a saída de som do `cpal` no Android, chega no 26.
        minSdk = 26
        targetSdk = 37
        versionCode = 1
        versionName = "0.1.0-dev1"
        testInstrumentationRunner = "io.github.brew.handset.NativeUiTests"
        ndk {
            abiFilters += "arm64-v8a"
        }
    }

    buildTypes {
        release {
            isMinifyEnabled = false
        }
    }

    packaging {
        jniLibs {
            // O `.so` fica descompactado e alinhado à página, que é o que o Android 15 pede e
            // o que deixa o sistema mapeá-lo direto do APK, sem cópia.
            useLegacyPackaging = false
        }
    }
}
