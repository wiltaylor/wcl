package dev.wcl.jetbrains

import org.jetbrains.plugins.textmate.api.TextMateBundleProvider
import java.nio.file.Path

class WclTextMateBundleProvider : TextMateBundleProvider {
    override fun getBundles(): List<TextMateBundleProvider.PluginBundle> {
        val bundlePath = javaClass.getResource("/textmate")?.toURI()?.let { Path.of(it) }
            ?: return emptyList()
        return listOf(TextMateBundleProvider.PluginBundle("wcl-textmate", bundlePath))
    }
}
