using System;
using System.Collections.Generic;
using System.IO;
using System.Linq;

namespace Wcl.Library
{
    public static class LibraryManager
    {
        private static string GetLibraryDir()
        {
            var xdgDataHome = Environment.GetEnvironmentVariable("XDG_DATA_HOME");
            if (!string.IsNullOrEmpty(xdgDataHome))
                return Path.Combine(xdgDataHome, "wcl", "lib");

            var home = Environment.GetEnvironmentVariable("HOME");
            if (!string.IsNullOrEmpty(home))
                return Path.Combine(home, ".local", "share", "wcl", "lib");

            return Path.Combine(".wcl", "lib");
        }

        public static List<string> List()
        {
            var dir = GetLibraryDir();
            if (!Directory.Exists(dir))
                return new List<string>();

            return Directory.GetFiles(dir, "*.wcl")
                .OrderBy(p => p)
                .ToList();
        }
    }
}
