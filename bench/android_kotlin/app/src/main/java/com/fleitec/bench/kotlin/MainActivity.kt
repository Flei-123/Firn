package com.fleitec.bench.kotlin

import android.content.Intent
import android.graphics.Color as AndroidColor
import android.os.Bundle
import android.os.Process
import android.os.SystemClock
import android.util.Log
import android.view.ViewTreeObserver
import androidx.activity.ComponentActivity
import androidx.activity.SystemBarStyle
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.statusBars
import androidx.compose.foundation.layout.windowInsetsPadding
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.consumeWindowInsets
import androidx.compose.foundation.layout.safeDrawing
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.List
import androidx.compose.material.icons.filled.Build
import androidx.compose.material.icons.filled.Settings
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.NavigationBar
import androidx.compose.material3.NavigationBarItem
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateListOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.unit.dp
import java.util.Locale

/** Logcat tag read by the measure script. */
object BenchLog {
    const val TAG = "BENCH"
    fun i(message: String) {
        Log.i(TAG, message)
    }

    fun f(format: String, vararg args: Any?) = i(String.format(Locale.US, format, *args))
}

enum class Tab(val label: String, val icon: ImageVector) {
    List("List", Icons.AutoMirrored.Filled.List),
    Lab("Lab", Icons.Filled.Build),
    Settings("Settings", Icons.Filled.Settings),
}

enum class LabSection(val label: String) { Anim("Anim"), Text("Text"), Net("Net"), Cpu("CPU") }

/** Test hooks delivered via intent extras (see bench spec). */
data class LaunchConfig(
    val id: Int = 0,
    val tab: Tab? = null,
    val lab: LabSection? = null,
    val autorun: Boolean = false,
    val autoscroll: Boolean = false,
    val theme: ThemeMode? = null,
    val netBase: String? = null,
) {
    companion object {
        fun from(intent: Intent?, id: Int): LaunchConfig {
            if (intent == null) return LaunchConfig(id = id)
            return LaunchConfig(
                id = id,
                tab = Tab.entries.firstOrNull { it.name.equals(intent.getStringExtra("tab"), true) },
                lab = LabSection.entries.firstOrNull { it.name.equals(intent.getStringExtra("lab"), true) },
                autorun = intent.getBooleanExtra("autorun", false),
                autoscroll = intent.getIntExtra("autoscroll", 0) != 0,
                netBase = intent.getStringExtra("netbase"),
                theme = when (intent.getStringExtra("theme")?.lowercase(Locale.ROOT)) {
                    "light" -> ThemeMode.Light
                    "dark" -> ThemeMode.Dark
                    else -> null
                },
            )
        }
    }
}

class MainActivity : ComponentActivity() {
    private var launch by mutableStateOf(LaunchConfig())

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        launch = LaunchConfig.from(intent, 1)
        setContent {
            BenchApp(
                launch = launch,
                onAutorunConsumed = { launch = launch.copy(autorun = false) },
            )
        }
        logReadyAfterFirstFrame()
    }

    override fun onNewIntent(intent: Intent) {
        super.onNewIntent(intent)
        setIntent(intent)
        launch = LaunchConfig.from(intent, launch.id + 1)
    }

    /** `BENCH ready ms=<since process start>` once the first frame has been drawn. */
    private fun logReadyAfterFirstFrame() {
        val decor = window.decorView
        decor.viewTreeObserver.addOnDrawListener(object : ViewTreeObserver.OnDrawListener {
            private var done = false
            override fun onDraw() {
                if (done) return
                done = true
                decor.post {
                    val ms = SystemClock.elapsedRealtime() - Process.getStartElapsedRealtime()
                    BenchLog.i("ready ms=$ms")
                    decor.viewTreeObserver.removeOnDrawListener(this)
                    reportFullyDrawn()
                }
            }
        })
    }
}

@Composable
fun BenchApp(launch: LaunchConfig, onAutorunConsumed: () -> Unit) {
    var mode by rememberSaveable { mutableStateOf(launch.theme ?: ThemeMode.System) }
    var tab by rememberSaveable { mutableStateOf(launch.tab ?: Tab.List) }
    LaunchedEffect(launch.id) {
        launch.theme?.let { mode = it }
        launch.tab?.let { tab = it }
    }

    val items = remember { mutableStateListOf<Int>().apply { addAll(1..1000) } }
    val listState = rememberLazyListState()
    var sliderValue by rememberSaveable { mutableStateOf(0.5f) }

    BenchTheme(mode) {
        val dark = isDark(mode)
        val activity = androidx.compose.ui.platform.LocalContext.current as ComponentActivity
        DisposableEffect(dark) {
            val style = if (dark) {
                SystemBarStyle.dark(AndroidColor.TRANSPARENT)
            } else {
                SystemBarStyle.light(AndroidColor.TRANSPARENT, AndroidColor.TRANSPARENT)
            }
            activity.enableEdgeToEdge(statusBarStyle = style, navigationBarStyle = style)
            onDispose {}
        }

        Scaffold(
            containerColor = MaterialTheme.colorScheme.background,
            topBar = { BenchAppBar() },
            bottomBar = {
                NavigationBar {
                    Tab.entries.forEach { t ->
                        NavigationBarItem(
                            selected = tab == t,
                            onClick = { tab = t },
                            icon = { Icon(t.icon, contentDescription = t.label) },
                            label = { Text(t.label) },
                        )
                    }
                }
            },
        ) { padding ->
            val content = Modifier.padding(padding).consumeWindowInsets(padding).fillMaxSize()
            when (tab) {
                Tab.List -> ListScreen(
                    items = items,
                    listState = listState,
                    dark = dark,
                    onDarkChange = { mode = if (it) ThemeMode.Dark else ThemeMode.Light },
                    autoscroll = launch.autoscroll,
                    modifier = content,
                )
                Tab.Lab -> LabScreen(
                    launch = launch,
                    onAutorunConsumed = onAutorunConsumed,
                    modifier = content,
                )
                Tab.Settings -> SettingsScreen(
                    mode = mode,
                    onModeChange = { mode = it },
                    sliderValue = sliderValue,
                    onSliderChange = { sliderValue = it },
                    modifier = content,
                )
            }
        }
    }
}

@Composable
private fun BenchAppBar() {
    Surface(color = MaterialTheme.colorScheme.surface) {
        Box(
            Modifier
                .windowInsetsPadding(WindowInsets.statusBars)
                .fillMaxWidth()
                .height(56.dp)
                .padding(horizontal = 16.dp),
            contentAlignment = Alignment.CenterStart,
        ) {
            Text("Bench", style = MaterialTheme.typography.titleLarge)
        }
    }
}
