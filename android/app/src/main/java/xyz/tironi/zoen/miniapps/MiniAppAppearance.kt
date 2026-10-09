package xyz.tironi.zoen.miniapps

/** Android tonal overlays avoid capturing and blurring the map behind every control. */
internal object MiniAppAppearance {
    fun variables(dark: Boolean): Map<String, String> = linkedMapOf(
        "--font-sans" to "Roboto, system-ui, sans-serif",
        "--color-background-primary" to if (dark) "#191C18" else "#FCFAF4",
        "--color-background-secondary" to if (dark) "#252A22" else "#F0EEE6",
        "--color-text-primary" to if (dark) "#ECEFE3" else "#1C2119",
        "--color-text-secondary" to if (dark) "#BDC7B5" else "#687062",
        "--color-border-primary" to if (dark) "#424A3B" else "#D8DCCF",
        "--z-glass-filter" to "none",
        "--z-glass" to if (dark) "#252A22" else "#F0EEE6",
        "--z-glass-stroke" to if (dark) "#424A3B" else "#D8DCCF",
        "--z-header-background" to if (dark) "#252A22" else "#F0EEE6",
    )

    /** Apply before application code can create its first backdrop-filter surface. */
    fun initialScript(dark: Boolean): String {
        val assignments = variables(dark).entries.joinToString("\n") { (name, value) ->
            "root.style.setProperty('$name', '$value');"
        }
        return """
            (() => {
              const root = document.documentElement;
              root.dataset.theme = '${if (dark) "dark" else "light"}';
              $assignments
            })();
        """.trimIndent()
    }
}
