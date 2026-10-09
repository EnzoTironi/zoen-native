plugins {
    alias(libs.plugins.android.application)
    alias(libs.plugins.compose.compiler)
    alias(libs.plugins.kotlin.serialization)
}

val coreAbis = providers.gradleProperty("androidAbis").orElse("arm64-v8a,x86_64")
val coreProfile = providers.gradleProperty("coreProfile").orElse("dev")
val coreOutput = layout.buildDirectory.dir("generated/roda")
val buildRodaCore by tasks.registering(Exec::class) {
    group = "build"
    description = "Build the shared Rust engine and generate UniFFI Kotlin bindings."
    workingDir(rootProject.projectDir.parentFile)
    commandLine("bash", "scripts/build-android-core.sh",
        if (coreProfile.get() == "release") "--release" else "--debug", "--abis=${coreAbis.get()}")
    inputs.files(fileTree("../../crates") { include("**/*.rs", "**/Cargo.toml", "**/*.html", "**/*.json") })
    inputs.files("../../Cargo.toml", "../../Cargo.lock", "../uniffi.toml", "../../scripts/build-android-core.sh")
    inputs.property("abis", coreAbis)
    inputs.property("profile", coreProfile)
    outputs.dir(coreOutput)
}

android {
    namespace = "xyz.tironi.zoen"
    compileSdk = 36
    ndkVersion = "27.1.12297006"
    defaultConfig {
        applicationId = "xyz.tironi.zoen"
        minSdk = 28
        targetSdk = 36
        versionCode = 1
        versionName = "0.1.0"
        testInstrumentationRunner = "androidx.test.runner.AndroidJUnitRunner"
        ndk { abiFilters += coreAbis.get().split(",") }
        buildConfigField("String", "RELAY_URL", "\"https://relay.tryzoen.com\"")
    }
    sourceSets {
        getByName("main") {
            kotlin.srcDir(coreOutput.get().dir("kotlin").asFile)
            jniLibs.srcDir(coreOutput.get().dir("jniLibs").asFile)
        }
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
    buildFeatures { compose = true; buildConfig = true }
    packaging {
        resources.excludes += "/META-INF/{AL2.0,LGPL2.1}"
        jniLibs.useLegacyPackaging = false
    }
}

kotlin { jvmToolchain(17) }
tasks.named("preBuild") { dependsOn(buildRodaCore) }

dependencies {
    implementation(platform(libs.androidx.compose.bom))
    androidTestImplementation(platform(libs.androidx.compose.bom))
    implementation(libs.androidx.core.ktx)
    implementation(libs.androidx.activity.compose)
    implementation(libs.androidx.lifecycle.runtime.compose)
    implementation(libs.androidx.lifecycle.viewmodel.compose)
    implementation(libs.androidx.compose.ui)
    implementation(libs.androidx.compose.material3)
    implementation(libs.androidx.navigation3.runtime)
    implementation(libs.androidx.navigation3.ui)
    implementation("androidx.compose.material:material-icons-extended:1.7.8")
    implementation("net.java.dev.jna:jna:5.18.1@aar")
    implementation("androidx.webkit:webkit:1.16.0")
    implementation("androidx.exifinterface:exifinterface:1.4.2")
    implementation("com.google.mlkit:genai-prompt:1.0.0-beta2")
    implementation("org.jetbrains.kotlinx:kotlinx-serialization-json:1.10.0")
    implementation("androidx.media3:media3-transformer:1.9.3")
    implementation("com.tom-roush:pdfbox-android:2.0.27.0")
    debugImplementation(libs.androidx.compose.ui.tooling)
    debugImplementation(libs.androidx.compose.ui.test.manifest)
    testImplementation(libs.junit)
    testImplementation(libs.kotlinx.coroutines.test)
    androidTestImplementation(libs.androidx.compose.ui.test.junit4)
    androidTestImplementation(libs.androidx.test.core)
    androidTestImplementation(libs.androidx.test.ext.junit)
    androidTestImplementation(libs.androidx.test.runner)
}
