package com.fleitec.bench.kotlin

import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.material3.ColorScheme
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.darkColorScheme
import androidx.compose.material3.lightColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.ui.graphics.Color

enum class ThemeMode { System, Light, Dark }

// Jarvis tokens (see bench spec).
private val Bg = Color(0xFF0C0E1F)
private val Panel = Color(0xFF15172B)
private val Txt = Color(0xFFE9EBF5)
private val Dim = Color(0xFF9AA0BB)
private val Accent = Color(0xFF2F86FF)

val DarkScheme: ColorScheme = darkColorScheme(
    primary = Accent,
    onPrimary = Color.White,
    secondaryContainer = Color(0xFF1F3A66),
    onSecondaryContainer = Txt,
    background = Bg,
    onBackground = Txt,
    surface = Panel,
    onSurface = Txt,
    surfaceVariant = Panel,
    onSurfaceVariant = Dim,
    surfaceContainerLowest = Bg,
    surfaceContainerLow = Panel,
    surfaceContainer = Panel,
    surfaceContainerHigh = Color(0xFF1C1F38),
    surfaceContainerHighest = Color(0xFF232743),
    outline = Color(0xFF3A3F63),
    outlineVariant = Color(0xFF2A2E4D),
)

val LightScheme: ColorScheme = lightColorScheme(
    primary = Accent,
    onPrimary = Color.White,
    secondaryContainer = Color(0xFFD6E6FF),
    onSecondaryContainer = Color(0xFF141830),
    background = Color(0xFFF4F6FB),
    onBackground = Color(0xFF141830),
    surface = Color.White,
    onSurface = Color(0xFF141830),
    surfaceVariant = Color.White,
    onSurfaceVariant = Color(0xFF5A6283),
    surfaceContainerLowest = Color(0xFFF4F6FB),
    surfaceContainerLow = Color.White,
    surfaceContainer = Color.White,
    surfaceContainerHigh = Color(0xFFEBEEF7),
    surfaceContainerHighest = Color(0xFFE2E6F2),
    outline = Color(0xFFB9BFD6),
    outlineVariant = Color(0xFFD9DDEC),
)

@Composable
fun isDark(mode: ThemeMode): Boolean = when (mode) {
    ThemeMode.System -> isSystemInDarkTheme()
    ThemeMode.Light -> false
    ThemeMode.Dark -> true
}

@Composable
fun BenchTheme(mode: ThemeMode, content: @Composable () -> Unit) {
    MaterialTheme(colorScheme = if (isDark(mode)) DarkScheme else LightScheme, content = content)
}
