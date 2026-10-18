package com.fleitec.bench.kotlin

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.LazyListState
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Star
import androidx.compose.material3.AssistChip
import androidx.compose.material3.Button
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.material3.TextField
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.derivedStateOf
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.runtime.snapshots.SnapshotStateList
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.unit.dp
import kotlin.math.abs
import kotlin.math.max

private val IconColors = listOf(
    Color(0xFF2F86FF),
    Color(0xFF22C55E),
    Color(0xFFF59E0B),
    Color(0xFFEF4444),
    Color(0xFFA855F7),
)

/**
 * List tab: filter field, Add/Clear buttons, dark switch and 1000 cards.
 * Add appends one item at the end; Clear empties the filter field.
 */
@Composable
fun ListScreen(
    items: SnapshotStateList<Int>,
    listState: LazyListState,
    dark: Boolean,
    onDarkChange: (Boolean) -> Unit,
    autoscroll: Boolean,
    modifier: Modifier = Modifier,
) {
    var query by rememberSaveable { mutableStateOf("") }
    val shown by remember(query) {
        derivedStateOf {
            if (query.isEmpty()) items.toList() else items.filter { "Item $it".contains(query, ignoreCase = true) }
        }
    }

    Column(modifier) {
        TextField(
            value = query,
            onValueChange = { query = it },
            placeholder = { Text("Filter") },
            singleLine = true,
            modifier = Modifier.fillMaxWidth().padding(horizontal = 16.dp, vertical = 8.dp),
        )
        Row(
            Modifier.fillMaxWidth().padding(horizontal = 16.dp),
            horizontalArrangement = Arrangement.spacedBy(8.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Button(onClick = { items.add((items.lastOrNull() ?: 0) + 1) }) { Text("Add") }
            OutlinedButton(onClick = { query = "" }) { Text("Clear") }
            Spacer(Modifier.weight(1f))
            Text("Dark")
            Switch(checked = dark, onCheckedChange = onDarkChange)
        }
        LazyColumn(
            state = listState,
            contentPadding = PaddingValues(16.dp),
            verticalArrangement = Arrangement.spacedBy(8.dp),
            modifier = Modifier.weight(1f),
        ) {
            items(shown, key = { it }) { n -> ItemCard(n) }
        }
    }

    if (autoscroll) AutoScroll(listState)
}

@Composable
private fun ItemCard(n: Int) {
    Card(
        shape = RoundedCornerShape(16.dp),
        colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.surface),
        modifier = Modifier.fillMaxWidth(),
    ) {
        Row(
            Modifier.padding(12.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Icon(
                Icons.Filled.Star,
                contentDescription = null,
                tint = IconColors[n % 5],
                modifier = Modifier.size(24.dp),
            )
            Spacer(Modifier.width(12.dp))
            Column(Modifier.weight(1f)) {
                Text("Item $n", style = MaterialTheme.typography.titleMedium)
                Text(
                    "Subtitle for item $n — lorem ipsum",
                    style = MaterialTheme.typography.bodyMedium,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
            }
            Spacer(Modifier.width(8.dp))
            AssistChip(onClick = {}, label = { Text("tag ${n % 7}") })
        }
    }
}

/** Scrolls 0 -> end at constant speed over 5 s, then back, repeated (test hook). */
@Composable
private fun AutoScroll(state: LazyListState) {
    LaunchedEffect(state) {
        var direction = 1f
        var speed = 0f // px per second
        var last = androidx.compose.runtime.withFrameNanos { it }
        while (true) {
            val now = androidx.compose.runtime.withFrameNanos { it }
            val dt = (now - last) / 1_000_000_000f
            last = now
            if (speed == 0f) {
                val info = state.layoutInfo
                val first = info.visibleItemsInfo.firstOrNull() ?: continue
                val viewport = (info.viewportEndOffset - info.viewportStartOffset).toFloat()
                val total = (first.size + info.mainAxisItemSpacing) * info.totalItemsCount - viewport
                speed = max(1f, total) / 5f
            }
            val delta = direction * speed * dt
            val consumed = state.scrollBy(delta)
            if (abs(consumed) < abs(delta) - 0.5f) direction = -direction
        }
    }
}
