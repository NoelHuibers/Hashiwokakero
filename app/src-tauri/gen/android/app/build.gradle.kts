import java.util.Properties
import org.jetbrains.kotlin.gradle.dsl.JvmTarget

plugins {
    id("com.android.application")
    id("org.jetbrains.kotlin.android")
    id("rust")
}

val tauriProperties = Properties().apply {
    val propFile = file("tauri.properties")
    if (propFile.exists()) {
        propFile.inputStream().use { load(it) }
    }
}

// The upload key that signs release builds for Google Play (app/README.md, Releases): from
// keystore.properties next to settings.gradle (gitignored), else from the environment. Without
// one, release builds stay unsigned. Debug builds always use the local debug key.
val keystoreProperties = Properties().apply {
    val propFile = rootProject.file("keystore.properties")
    if (propFile.exists()) {
        propFile.inputStream().use { load(it) }
    }
}
fun signingValue(property: String, variable: String): String? =
    keystoreProperties.getProperty(property)
        ?: providers.environmentVariable(variable).orNull?.takeIf { it.isNotEmpty() }
val uploadKeystore = signingValue("storeFile", "ANDROID_KEYSTORE_FILE")

// Google Play Games (app/README.md, Games services): the numeric project id of the game's Play
// Games Services project in the Play Console, which isn't secret. Release builds take
// `project_id` of app/achievements-play.json (with Play's ids of the achievements), every build
// HASHI_PLAY_GAMES_APP_ID of the environment instead (debug builds are rs.hashi.debug, which Play
// Games only signs in with a credential of their own). Empty, null or no file: no Play Games, the
// SDK is never started (its start-up provider is off, HashiPlugin answers that the service isn't
// there).
val playIds = providers.fileContents(
    rootProject.layout.projectDirectory.file("../../../achievements-play.json"),
).asText
fun playGamesAppId(release: Boolean): String {
    val id = providers.environmentVariable("HASHI_PLAY_GAMES_APP_ID").orNull
        ?: if (release) {
            playIds.orNull
                ?.let { (groovy.json.JsonSlurper().parseText(it) as? Map<*, *>)?.get("project_id") }
                ?.toString()
        } else {
            null
        }
    return id.orEmpty().trim().also {
        require(it.all(Char::isDigit)) { "the Play Games project id is a number, not '$it'" }
    }
}
fun com.android.build.api.dsl.ApplicationBuildType.playGames(release: Boolean) {
    val id = playGamesAppId(release)
    // The manifest's com.google.android.gms.games.APP_ID, a string: as a number Android would
    // read it as an integer, which the SDK doesn't take
    resValue("string", "game_services_project_id", id)
    manifestPlaceholders["playGamesEnabled"] = id.isNotEmpty().toString()
    buildConfigField("String", "PLAY_GAMES_APP_ID", "\"$id\"")
}

android {
    compileSdk = 37
    namespace = "rs.hashi"
    // The NDK that Tauri links the library with (NDK_HOME, NDK_VERSION in the workflows).
    // Release builds strip the library with it and take its symbol table out; with Gradle's own
    // default version, which isn't installed, it packs the library unstripped and without
    // symbols for Play.
    ndkVersion = "29.0.14206865"
    defaultConfig {
        manifestPlaceholders["usesCleartextTraffic"] = "false"
        applicationId = "rs.hashi"
        minSdk = 24
        targetSdk = 37
        versionCode = tauriProperties.getProperty("tauri.android.versionCode", "1").toInt()
        versionName = tauriProperties.getProperty("tauri.android.versionName", "1.0")
    }
    signingConfigs {
        if (uploadKeystore != null) {
            create("upload") {
                storeFile = rootProject.file(uploadKeystore)
                storePassword = signingValue("storePassword", "ANDROID_KEYSTORE_PASSWORD")
                keyAlias = signingValue("keyAlias", "ANDROID_KEY_ALIAS")
                keyPassword = signingValue("keyPassword", "ANDROID_KEY_PASSWORD")
            }
        }
    }
    buildTypes {
        getByName("debug") {
            applicationIdSuffix = ".debug"
            manifestPlaceholders["usesCleartextTraffic"] = "true"
            playGames(release = false)
            isDebuggable = true
            isJniDebuggable = true
            isMinifyEnabled = false
            packaging {
                jniLibs.keepDebugSymbols.add("*/arm64-v8a/*.so")
                jniLibs.keepDebugSymbols.add("*/armeabi-v7a/*.so")
                jniLibs.keepDebugSymbols.add("*/x86/*.so")
                jniLibs.keepDebugSymbols.add("*/x86_64/*.so")
            }
        }
        getByName("release") {
            signingConfigs.findByName("upload")?.let { signingConfig = it }
            playGames(release = true)
            // The native library's symbol table in the App Bundle (BUNDLE-METADATA), from which
            // Play symbolicates native crashes. Devices get the stripped library.
            ndk {
                debugSymbolLevel = "SYMBOL_TABLE"
            }
            optimization {
               enable = true
            }
            proguardFiles(
                *fileTree(".") {
                  include("**/*.pro")
                  exclude("build/**")
                }.files.toTypedArray()
            )
        }
    }
    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_1_8
        targetCompatibility = JavaVersion.VERSION_1_8
    }
    buildFeatures {
        buildConfig = true
        // The Play Games project id's string (playGames above)
        resValues = true
    }
}

kotlin {
    compilerOptions {
        jvmTarget = JvmTarget.JVM_1_8
    }
}

// The debug build type's keepDebugSymbols above lands in the packaging of every variant, which
// kept Gradle from stripping release libraries and taking their symbols. Release keeps none.
androidComponents {
    onVariants(selector().withBuildType("release")) { variant ->
        variant.packaging.jniLibs.keepDebugSymbols.set(emptySet())
    }
}

rust {
    rootDirRel = "../../../"
}

dependencies {
    implementation("androidx.webkit:webkit:1.18.0")
    implementation("androidx.appcompat:appcompat:1.8.0")
    implementation("androidx.activity:activity-ktx:1.10.1")
    implementation("com.google.android.material:material:1.14.0")
    implementation("androidx.lifecycle:lifecycle-process:2.10.0")
    // Google Play Games Services v2: sign-in, achievements, leaderboards (HashiPlugin.kt)
    implementation("com.google.android.gms:play-services-games-v2:22.1.0")
    testImplementation("junit:junit:4.13.2")
    androidTestImplementation("androidx.test.ext:junit:1.1.4")
    androidTestImplementation("androidx.test.espresso:espresso-core:3.5.0")
}

apply(from = file("tauri.build.gradle.kts"))
