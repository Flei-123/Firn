package com.fleitec.bench.kotlin

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Button
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.SegmentedButton
import androidx.compose.material3.SegmentedButtonDefaults
import androidx.compose.material3.SingleChoiceSegmentedButtonRow
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.runtime.withFrameNanos
import androidx.compose.animation.core.LinearEasing
import androidx.compose.animation.core.RepeatMode
import androidx.compose.animation.core.animateFloat
import androidx.compose.animation.core.infiniteRepeatable
import androidx.compose.animation.core.rememberInfiniteTransition
import androidx.compose.animation.core.tween
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import org.json.JSONObject
import java.io.IOException
import java.net.HttpURLConnection
import java.net.URL

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun LabScreen(launch: LaunchConfig, onAutorunConsumed: () -> Unit, modifier: Modifier = Modifier) {
    var section by rememberSaveable { mutableStateOf(launch.lab ?: LabSection.Anim) }
    LaunchedEffect(launch.id) { launch.lab?.let { section = it } }

    val net = remember { NetState() }
    val cpu = remember { CpuState() }
    val scope = rememberCoroutineScope()

    Column(modifier) {
        SingleChoiceSegmentedButtonRow(Modifier.fillMaxWidth().padding(16.dp)) {
            LabSection.entries.forEachIndexed { index, s ->
                SegmentedButton(
                    selected = section == s,
                    onClick = { section = s },
                    shape = SegmentedButtonDefaults.itemShape(index, LabSection.entries.size),
                ) { Text(s.label) }
            }
        }
        // Autorun starts the task of the selected section exactly once. The task runs in the screen scope:
        // onAutorunConsumed() changes the effect keys, which would cancel a task started inside the effect.
        LaunchedEffect(launch.autorun, section) {
            if (!launch.autorun) return@LaunchedEffect
            when (section) {
                LabSection.Net -> { onAutorunConsumed(); scope.launch { net.run() } }
                LabSection.Cpu -> { onAutorunConsumed(); scope.launch { cpu.run() } }
                else -> Unit
            }
        }
        when (section) {
            LabSection.Anim -> AnimSection()
            LabSection.Text -> TextSection()
            LabSection.Net -> NetSection(net) { scope.launch { net.run() } }
            LabSection.Cpu -> CpuSection(cpu) { scope.launch { cpu.run() } }
        }
    }
}

// ---------------------------------------------------------------- Anim

/** Frame time window in microseconds; sorted on demand for percentiles. */
private class FrameWindow {
    private var data = LongArray(1024)
    var count = 0
        private set

    fun add(micros: Long) {
        if (count == data.size) data = data.copyOf(count * 2)
        data[count++] = micros
    }

    fun clear() { count = 0 }
    fun maxMs(): Double = if (count == 0) 0.0 else data.take(count).max() / 1000.0
    fun p95Ms(): Double {
        if (count == 0) return 0.0
        val sorted = data.copyOf(count).also { it.sort() }
        return sorted[minOf(count - 1, (count * 0.95).toInt())] / 1000.0
    }
}

@Composable
fun AnimSection() {
    val transition = rememberInfiniteTransition(label = "spin")
    val angle by transition.animateFloat(
        initialValue = 0f,
        targetValue = 360f,
        animationSpec = infiniteRepeatable(tween(2000, easing = LinearEasing), RepeatMode.Restart),
        label = "angle",
    )
    var fpsText by remember { mutableStateOf("fps: --") }
    var frameText by remember { mutableStateOf("frame ms p95: -- max: --") }

    LaunchedEffect(Unit) {
        val second = FrameWindow()
        val window = FrameWindow()
        var last = 0L
        var secondStart = 0L
        var windowStart = 0L
        var windowFrames = 0
        while (true) {
            val now = withFrameNanos { it }
            if (last == 0L) {
                secondStart = now
                windowStart = now
            } else {
                val micros = (now - last) / 1000
                second.add(micros)
                window.add(micros)
                windowFrames++
            }
            last = now
            if (now - secondStart >= 1_000_000_000L) {
                fpsText = "fps: ${second.count}"
                frameText = String.format(java.util.Locale.US, "frame ms p95: %.1f max: %.1f", second.p95Ms(), second.maxMs())
                second.clear()
                secondStart = now
            }
            if (now - windowStart >= 5_000_000_000L) {
                val avg = windowFrames / ((now - windowStart) / 1e9)
                BenchLog.f(
                    "fps avg=%.1f p95_ms=%.2f max_ms=%.2f frames=%d",
                    avg, window.p95Ms(), window.maxMs(), windowFrames,
                )
                window.clear()
                windowFrames = 0
                windowStart = now
            }
        }
    }

    Column(
        Modifier.fillMaxSize().padding(16.dp),
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.spacedBy(24.dp, Alignment.CenterVertically),
    ) {
        Box(
            Modifier
                .size(160.dp)
                .graphicsLayer { rotationZ = angle }
                .background(MaterialTheme.colorScheme.primary, RoundedCornerShape(24.dp)),
        )
        Text(fpsText, style = MaterialTheme.typography.titleLarge)
        Text(frameText, style = MaterialTheme.typography.bodyMedium, color = MaterialTheme.colorScheme.onSurfaceVariant)
    }
}

// ---------------------------------------------------------------- Text

@Composable
fun TextSection() {
    LazyColumn(
        Modifier.fillMaxSize(),
        contentPadding = PaddingValues(16.dp),
    ) {
        items((1..400).toList()) { n ->
            Text(
                "Line $n: The quick brown fox jumps over the lazy dog 0123456789",
                fontSize = if (n % 2 == 1) 14.sp else 18.sp,
            )
        }
    }
}

// ---------------------------------------------------------------- Net

class NetState {
    var text by mutableStateOf("press GET")
    var running by mutableStateOf(false)

    suspend fun run() {
        if (running) return
        running = true
        text = "requesting..."
        val r = withContext(Dispatchers.IO) { httpGet() }
        text = if (r.status < 0) {
            "status: error"
        } else {
            "status: ${r.status}\nbytes: ${r.bytes}\nelapsed: ${r.ms} ms\nkeys: ${r.keys ?: "-"}"
        }
        BenchLog.i("net status=${r.status} bytes=${r.bytes} ms=${r.ms}")
        running = false
    }
}

private class NetResult(val status: Int, val bytes: Int, val ms: Long, val keys: Int?)

private fun httpGet(): NetResult {
    var last = NetResult(-1, 0, 0, null)
    for (url in listOf("https://httpbin.org/json", "https://example.com/")) {
        val t0 = System.nanoTime()
        try {
            val conn = URL(url).openConnection() as HttpURLConnection
            conn.connectTimeout = 8000
            conn.readTimeout = 8000
            val status = conn.responseCode
            val stream = if (status < 400) conn.inputStream else conn.errorStream
            val body = stream?.use { it.readBytes() } ?: ByteArray(0)
            val type = conn.contentType.orEmpty()
            conn.disconnect()
            val keys = if ("json" in type) runCatching { JSONObject(String(body)).length() }.getOrNull() else null
            last = NetResult(status, body.size, (System.nanoTime() - t0) / 1_000_000, keys)
            if (status in 200..299) return last
        } catch (_: IOException) {
            // try the fallback URL
        }
    }
    return last
}

@Composable
fun NetSection(state: NetState, onGet: () -> Unit) {
    Column(Modifier.fillMaxSize().padding(16.dp), verticalArrangement = Arrangement.spacedBy(16.dp)) {
        Button(onClick = onGet, enabled = !state.running) { Text("GET") }
        Text(state.text, style = MaterialTheme.typography.bodyLarge)
    }
}

// ---------------------------------------------------------------- CPU

class CpuState {
    var running by mutableStateOf(false)
    var primes by mutableStateOf("primes: -")
    var sort by mutableStateOf("sort: -")
    var mandel by mutableStateOf("mandel: -")

    /** Runs the three tasks one after another on Dispatchers.Default. */
    suspend fun run() {
        if (running) return
        running = true
        primes = "primes: ..."
        sort = "sort: ..."
        mandel = "mandel: ..."
        val p = timed { Cpu.primes() }
        primes = "primes: ${p.first} ms, checksum ${p.second}"
        val s = timed { Cpu.sort() }
        sort = "sort: ${s.first} ms, checksum ${s.second}"
        val m = timed { Cpu.mandelbrot() }
        mandel = "mandel: ${m.first} ms, checksum ${m.second}"
        BenchLog.i("cpu primes=${p.first}:${p.second} sort=${s.first}:${s.second} mandel=${m.first}:${m.second}")
        running = false
    }

    private suspend fun timed(block: () -> Long): Pair<Long, Long> = withContext(Dispatchers.Default) {
        val t0 = System.nanoTime()
        val checksum = block()
        ((System.nanoTime() - t0) / 1_000_000) to checksum
    }
}

@Composable
fun CpuSection(state: CpuState, onRun: () -> Unit) {
    Column(Modifier.fillMaxSize().padding(16.dp), verticalArrangement = Arrangement.spacedBy(16.dp)) {
        Button(onClick = onRun, enabled = !state.running) { Text("Run") }
        Text(state.primes, style = MaterialTheme.typography.bodyLarge)
        Text(state.sort, style = MaterialTheme.typography.bodyLarge)
        Text(state.mandel, style = MaterialTheme.typography.bodyLarge)
    }
}
