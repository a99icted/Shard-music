plugins {
    id("com.android.application")
    id("org.jetbrains.kotlin.android")
    id("org.jetbrains.kotlin.plugin.compose")
}

android {
    namespace = "music.shard.mobile"
    compileSdk = 35
    ndkVersion = "27.2.12479018"
    defaultConfig {
        applicationId = "music.shard.mobile"
        minSdk = 26
        targetSdk = 35
        versionCode = 6
        versionName = "0.1.5"
        testInstrumentationRunner = "androidx.test.runner.AndroidJUnitRunner"
    }
    buildTypes {
        release {
            isMinifyEnabled = true
            isShrinkResources = true
            proguardFiles(getDefaultProguardFile("proguard-android-optimize.txt"), "proguard-rules.pro")
        }
    }
    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }
    kotlinOptions { jvmTarget = "17" }
    buildFeatures { compose = true; buildConfig = true }
    packaging {
        jniLibs.useLegacyPackaging = true // yt-dlp executes the extracted Android binaries.
        jniLibs.keepDebugSymbols += setOf("**/libffmpeg.zip.so", "**/libpython.zip.so") // data archives, not ELF binaries
        resources.excludes += setOf("META-INF/DEPENDENCIES", "META-INF/LICENSE*", "META-INF/NOTICE*")
    }
    splits {
        abi {
            isEnable = true
            reset()
            include("arm64-v8a", "x86_64")
            isUniversalApk = false
        }
    }
}

val buildRust by tasks.registering(Exec::class) {
    workingDir = file("../../native")
    environment("ANDROID_NDK_HOME", "${android.sdkDirectory}/ndk/${android.ndkVersion}")
    environment("RUSTFLAGS", "-C link-arg=-Wl,-z,max-page-size=16384")
    commandLine("cargo", "ndk", "-t", "arm64-v8a", "-t", "x86_64", "--platform", "26",
        "-o", "../android/app/src/main/jniLibs", "build", "--locked", "--release")
    inputs.files(fileTree("../../../src"), fileTree("../../native/src"), file("../../native/Cargo.toml"), file("../../native/Cargo.lock"), file("../../../Cargo.toml"))
    outputs.files(file("src/main/jniLibs/arm64-v8a/libshard_mobile.so"), file("src/main/jniLibs/x86_64/libshard_mobile.so"))
}
tasks.named("preBuild") { dependsOn(buildRust) }

dependencies {
    implementation(platform("androidx.compose:compose-bom:2025.02.00"))
    implementation("androidx.activity:activity-compose:1.10.1")
    implementation("androidx.compose.material3:material3")
    implementation("androidx.compose.material:material-icons-extended")
    implementation("androidx.lifecycle:lifecycle-runtime-compose:2.8.7")
    implementation("androidx.lifecycle:lifecycle-viewmodel-compose:2.8.7")
    implementation("androidx.media3:media3-exoplayer:1.5.1")
    implementation("androidx.media3:media3-session:1.5.1")
    implementation("androidx.work:work-runtime-ktx:2.10.0")
    implementation("io.coil-kt:coil-compose:2.7.0")
    implementation("io.github.junkfood02.youtubedl-android:library:0.18.1")
    implementation("io.github.junkfood02.youtubedl-android:ffmpeg:0.18.1")
    testImplementation("junit:junit:4.13.2")
    testImplementation("org.json:json:20250107")
    androidTestImplementation("androidx.test.ext:junit:1.2.1")
    androidTestImplementation("androidx.test:runner:1.6.2")
    androidTestImplementation(platform("androidx.compose:compose-bom:2025.02.00"))
    androidTestImplementation("androidx.compose.ui:ui-test-junit4")
    debugImplementation("androidx.compose.ui:ui-test-manifest")
}
