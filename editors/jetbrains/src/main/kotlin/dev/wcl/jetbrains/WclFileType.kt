package dev.wcl.jetbrains

import com.intellij.openapi.fileTypes.LanguageFileType
import javax.swing.Icon

object WclFileType : LanguageFileType(WclLanguage) {
    override fun getName(): String = "WCL"
    override fun getDescription(): String = "WCL configuration file"
    override fun getDefaultExtension(): String = "wcl"
    override fun getIcon(): Icon = WclIcons.FILE
}
