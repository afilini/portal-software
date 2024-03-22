import org.jetbrains.kotlin.gradle.tasks.KotlinCompile

// library version is defined in gradle.properties
val libraryVersion: String by project
extra["isReleaseVersion"] = !libraryVersion.toString().endsWith("SNAPSHOT")

plugins {
    id("com.android.library")
    id("org.jetbrains.kotlin.android") version "1.6.10"
    id("maven-publish")
    id("signing")

    // Custom plugin to generate the native libs and bindings file
    id("xyz.twenty_two.plugins.generate-android-bindings")
}

repositories {
    mavenCentral()
    google()
}

android {
    compileSdk = 33

    defaultConfig {
        minSdk = 21
        targetSdk = 33
        testInstrumentationRunner = "androidx.test.runner.AndroidJUnitRunner"
        consumerProguardFiles("consumer-rules.pro")
    }

    buildTypes {
        getByName("release") {
            isMinifyEnabled = false
            proguardFiles(file("proguard-android-optimize.txt"), file("proguard-rules.pro"))
        }
    }

    publishing {
        singleVariant("release") {
            withSourcesJar()
            withJavadocJar()
        }
    }
}

dependencies {
    implementation("net.java.dev.jna:jna:5.14.0@aar")
    implementation("org.jetbrains.kotlin:kotlin-stdlib-jdk7")
    implementation("androidx.appcompat:appcompat:1.4.0")
    implementation("androidx.core:core-ktx:1.7.0")
    implementation("org.jetbrains.kotlinx:kotlinx-coroutines-core:1.6.4")
    api("org.slf4j:slf4j-api:1.7.30")

    androidTestImplementation("com.github.tony19:logback-android:2.0.0")
    androidTestImplementation("androidx.test.ext:junit:1.1.3")
    androidTestImplementation("androidx.test.espresso:espresso-core:3.4.0")
    androidTestImplementation("org.jetbrains.kotlin:kotlin-test:1.6.10")
    androidTestImplementation("org.jetbrains.kotlin:kotlin-test-junit:1.6.10")
}

afterEvaluate {
    publishing {
        publications {
            create<MavenPublication>("maven") {
                groupId = "xyz.twenty-two"
                artifactId = "libportal-android"
                version = libraryVersion

                from(components["release"])
                pom {
                    name.set("libportal-android")
                    description.set("Android bindings for libportal")
                    url.set("https://twenty-two.xyz")
                    licenses {
                        license {
                            name.set("APACHE 2.0")
                            url.set("https://github.com/TwentyTwoHW/portal-software/blob/master/sdk/LICENSE-APACHE")
                        }
                        license {
                            name.set("MIT")
                            url.set("https://github.com/TwentyTwoHW/portal-software/blob/master/sdk/LICENSE-MIT")
                        }
                    }
                    scm {
                        connection.set("scm:git:github.com/TwentyTwoHW/portal-software.git")
                        developerConnection.set("scm:git:ssh://github.com/TwentyTwoHW/portal-software.git")
                        url.set("https://github.com/TwentyTwoHW/portal-software/tree/master")
                    }
                }
            }
        }
    }
}

signing {
    setRequired({
        (project.extra["isReleaseVersion"] as Boolean) && gradle.taskGraph.hasTask("publish")
    })
    useGpgCmd()
    sign(publishing.publications)
}

// This task dependency ensures that we build the bindings
// binaries before running the tests
tasks.withType<KotlinCompile> {
    dependsOn("buildAndroidLib")
}
