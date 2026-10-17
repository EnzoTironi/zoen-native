#!/usr/bin/env bash
# Focused Compose instrumentation using actual production/test source files, no Rust/JNI.
set -euo pipefail
cd "$(dirname "$0")/.."
: "${ANDROID_HOME:=$HOME/Library/Android/sdk}"
: "${ANDROID_SERIAL:=emulator-5556}"
export ANDROID_HOME ANDROID_SERIAL
harness=.reason/onboarding-harness
mkdir -p "$harness/src/main" "$harness/sources/main" "$harness/sources/androidTest"
cat > "$harness/settings.gradle.kts" <<'HARNESS_EOF'
pluginManagement { repositories { google(); mavenCentral(); gradlePluginPortal() } }
dependencyResolutionManagement { repositories { google(); mavenCentral() }; versionCatalogs { create("libs") { from(files("../../android/gradle/libs.versions.toml")) } } }
rootProject.name = "OnboardingHarness"
HARNESS_EOF
cat > "$harness/build.gradle.kts" <<'HARNESS_EOF'
plugins {
    alias(libs.plugins.android.application)
    alias(libs.plugins.compose.compiler)
}
android {
    namespace = "xyz.tironi.zoen.onboardingharness"
    compileSdk = 36
    defaultConfig {
        applicationId = "xyz.tironi.zoen.onboardingharness"
        minSdk = 28
        targetSdk = 36
        testInstrumentationRunner = "androidx.test.runner.AndroidJUnitRunner"
    }
    sourceSets {
        getByName("main") {
            kotlin.srcDir("sources/main")
        }
        getByName("androidTest") {
            kotlin.srcDir("sources/androidTest")
        }
    }
    compileOptions { sourceCompatibility = JavaVersion.VERSION_17; targetCompatibility = JavaVersion.VERSION_17 }
    buildFeatures { compose = true }
}
kotlin { jvmToolchain(17) }
dependencies {
    implementation(platform(libs.androidx.compose.bom))
    androidTestImplementation(platform(libs.androidx.compose.bom))
    implementation(libs.androidx.activity.compose)
    implementation(libs.androidx.lifecycle.runtime.compose)
    implementation(libs.androidx.lifecycle.viewmodel.compose)
    implementation(libs.androidx.navigation3.ui)
    implementation("com.google.guava:guava:33.3.1-android")
    implementation("androidx.concurrent:concurrent-futures:1.2.0")
    implementation("androidx.startup:startup-runtime:1.2.0")
    implementation("org.jetbrains.kotlinx:kotlinx-serialization-core:1.10.0")
    implementation(libs.androidx.compose.material3)
    debugImplementation(libs.androidx.compose.ui.test.manifest)
    androidTestImplementation(libs.androidx.compose.ui.test.junit4)
    androidTestImplementation(libs.androidx.test.ext.junit)
    androidTestImplementation(libs.androidx.test.runner)
}
HARNESS_EOF
cat > "$harness/src/main/AndroidManifest.xml" <<'HARNESS_EOF'
<manifest xmlns:android="http://schemas.android.com/apk/res/android"><application android:theme="@android:style/Theme.Material.Light.NoActionBar" /></manifest>
HARNESS_EOF
ln -sf ../../../../android/app/src/main/java/xyz/tironi/zoen/ui/OnboardingCompletion.kt "$harness/sources/main/OnboardingCompletion.kt"
ln -sf ../../../../android/app/src/androidTest/java/xyz/tironi/zoen/ui/OnboardingCompletionTest.kt "$harness/sources/androidTest/OnboardingCompletionTest.kt"
android/gradlew -p "$harness" connectedDebugAndroidTest --max-workers=2 -Dorg.gradle.jvmargs=-Xmx2g "$@"
