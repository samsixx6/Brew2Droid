plugins {
    id("com.android.application")
}

// Biblioteca e ajustes usam Views; a execução do jogo continua no .so.
android {
    namespace = "io.github.brew.handset"
    compileSdk { version = release(37) }

    defaultConfig {
        // O ID de instalação é independente do namespace usado pelas classes e pelo JNI.
        applicationId = "com.samsixx.brew2droid"
        // O AAudio, que é a saída de som do `cpal` no Android, chega no 26.
        minSdk = 26
        targetSdk = 37
        versionCode = 2
        versionName = "1.0"
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
