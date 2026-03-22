package dev.wcl.jetbrains.lsp

import com.intellij.openapi.project.Project
import com.redhat.devtools.lsp4ij.server.ProcessStreamConnectionProvider
import java.io.File
import java.nio.file.Paths

class WclLanguageServerDefinition(project: Project) : ProcessStreamConnectionProvider() {
    init {
        val binary = findWclBinary()
        super.setCommands(listOf(binary, "lsp"))
        project.basePath?.let { super.setWorkingDirectory(it) }
    }

    companion object {
        fun findWclBinary(): String {
            val isWindows = System.getProperty("os.name").lowercase().contains("win")
            val binName = if (isWindows) "wcl.exe" else "wcl"

            // 1. Bundled binary in plugin directory
            val pluginDir = findPluginDir()
            if (pluginDir != null) {
                val bundled = File(pluginDir, "bin/$binName")
                if (bundled.exists()) {
                    bundled.setExecutable(true)
                    return bundled.absolutePath
                }
            }

            // 2. Cargo bin fallback
            val home = System.getProperty("user.home")
            val cargoBin = Paths.get(home, ".cargo", "bin", binName).toFile()
            if (cargoBin.exists()) {
                return cargoBin.absolutePath
            }

            // 3. System PATH fallback
            return "wcl"
        }

        private fun findPluginDir(): File? {
            val url = WclLanguageServerDefinition::class.java
                .protectionDomain?.codeSource?.location ?: return null
            return try {
                val jarFile = File(url.toURI())
                // jar is in <plugin>/lib/xxx.jar, so parent.parent = plugin dir
                jarFile.parentFile?.parentFile
            } catch (_: Exception) {
                null
            }
        }
    }
}
