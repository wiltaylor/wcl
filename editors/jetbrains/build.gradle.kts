plugins {
    id("java")
    id("org.jetbrains.kotlin.jvm") version "2.1.20"
    id("org.jetbrains.intellij.platform") version "2.5.0"
}

group = providers.gradleProperty("pluginGroup").get()
version = providers.gradleProperty("pluginVersion").get()

repositories {
    mavenCentral()
    intellijPlatform {
        defaultRepositories()
        marketplace()
    }
}

dependencies {
    intellijPlatform {
        val platformType = providers.gradleProperty("platformType")
        val platformVersion = providers.gradleProperty("platformVersion")
        create(platformType, platformVersion)
        bundledPlugin("org.jetbrains.plugins.textmate")
        plugin("com.redhat.devtools.lsp4ij:0.19.2")
        pluginVerifier()
        zipSigner()
    }
}

java {
    sourceCompatibility = JavaVersion.VERSION_21
    targetCompatibility = JavaVersion.VERSION_21
}

tasks.withType<org.jetbrains.kotlin.gradle.tasks.KotlinCompile> {
    compilerOptions {
        jvmTarget.set(org.jetbrains.kotlin.gradle.dsl.JvmTarget.JVM_21)
    }
}

tasks {
    patchPluginXml {
        sinceBuild.set("242")
        untilBuild.set(provider { null })
    }

    buildSearchableOptions {
        enabled = false
    }

    prepareSandbox {
        val binDir = project.file("bin")
        if (binDir.exists()) {
            from(binDir) {
                into("${pluginName.get()}/bin")
            }
        }
    }
}

// Copy TextMate grammar from VS Code extension (single source of truth)
val syncTextMateGrammar by tasks.registering(Copy::class) {
    from("../vscode/syntaxes/wcl.tmLanguage.json")
    into("src/main/resources/textmate")
}

tasks.named("processResources") {
    dependsOn(syncTextMateGrammar)
}
