plugins {
    id("com.android.application")
}

android {
    namespace = "dev.nasaru.collector"
    compileSdk = 37

    defaultConfig {
        applicationId = "dev.nasaru.collector"
        minSdk = 30
        targetSdk = 37
        versionCode = 1
        versionName = "0.0.1"
    }

    buildFeatures {
        buildConfig = false
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }

    testOptions {
        unitTests.isReturnDefaultValues = true
    }
}

dependencies {
    testImplementation("junit:junit:4.13.2")
}
