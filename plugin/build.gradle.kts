plugins {
    id("java")
    id("org.jetbrains.kotlin.jvm") version "1.9.25"
    id("org.jetbrains.intellij.platform") version "2.18.1"
}

group = "dev.tare"
version = "0.1.0"

repositories {
    mavenCentral()
    intellijPlatform {
        defaultRepositories()
    }
}

dependencies {
    intellijPlatform {
        intellijIdeaCommunity("2024.3")

        pluginVerifier()
        zipSigner()
        instrumentationTools()
    }

    implementation("com.google.code.gson:gson:2.11.0")
    implementation("io.github.rctcwyvrn:blake3:1.3")
}

intellijPlatform {
    pluginConfiguration {
        id = "dev.tare.plugin"
        name = "Tare"
        version = project.version.toString()
        description = """
            Inline Rust memory allocation viewer. Annotates source lines with
            heap allocation data from runtime profiling (dhat) and static
            analysis (syn), surfaced as inlay hints and gutter icons.
        """.trimIndent()

        ideaVersion {
            sinceBuild = "243"
        }

        vendor {
            name = "tare"
        }
    }
}

kotlin {
    jvmToolchain(17)
}
