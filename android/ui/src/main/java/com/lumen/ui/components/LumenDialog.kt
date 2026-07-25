package com.lumen.ui.components

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import androidx.compose.ui.window.Dialog
import androidx.compose.ui.window.DialogProperties

/**
 * App-styled replacement for the stock Material AlertDialog: rounded card,
 * app surface colors and Lumen accent buttons.
 */
@Composable
fun LumenDialog(
    title: String,
    onDismissRequest: () -> Unit,
    message: String? = null,
    busy: Boolean = false,
    confirmText: String? = null,
    onConfirm: (() -> Unit)? = null,
    dismissText: String? = null,
    onDismiss: (() -> Unit)? = null,
    destructive: Boolean = false,
    content: (@Composable () -> Unit)? = null
) {
    Dialog(
        onDismissRequest = onDismissRequest,
        properties = DialogProperties(
            dismissOnBackPress = !busy,
            dismissOnClickOutside = !busy,
            usePlatformDefaultWidth = false
        )
    ) {
        Surface(
            modifier = Modifier
                .fillMaxWidth(0.88f)
                .clip(RoundedCornerShape(26.dp))
                .border(
                    1.dp,
                    MaterialTheme.colorScheme.outlineVariant.copy(alpha = 0.55f),
                    RoundedCornerShape(26.dp)
                ),
            color = MaterialTheme.colorScheme.surfaceVariant,
            shape = RoundedCornerShape(26.dp),
            tonalElevation = 0.dp
        ) {
            Column(Modifier.padding(horizontal = 22.dp, vertical = 20.dp)) {
                Text(
                    text = title,
                    style = MaterialTheme.typography.titleMedium,
                    fontWeight = FontWeight.Bold,
                    color = MaterialTheme.colorScheme.onSurface
                )

                if (busy) {
                    Spacer(Modifier.height(18.dp))
                    Box(Modifier.fillMaxWidth(), contentAlignment = Alignment.Center) {
                        CircularProgressIndicator(
                            modifier = Modifier.size(34.dp),
                            color = MaterialTheme.colorScheme.primary,
                            strokeWidth = 3.dp
                        )
                    }
                } else {
                    if (!message.isNullOrBlank()) {
                        Spacer(Modifier.height(10.dp))
                        Text(
                            text = message,
                            style = MaterialTheme.typography.bodyMedium,
                            color = MaterialTheme.colorScheme.onSurfaceVariant
                        )
                    }
                    if (content != null) {
                        Spacer(Modifier.height(14.dp))
                        content()
                    }
                }

                if (!busy && (confirmText != null || dismissText != null)) {
                    Spacer(Modifier.height(20.dp))
                    Row(
                        modifier = Modifier.fillMaxWidth(),
                        horizontalArrangement = Arrangement.spacedBy(10.dp)
                    ) {
                        if (dismissText != null) {
                            DialogButton(
                                label = dismissText,
                                filled = false,
                                modifier = Modifier.weight(1f),
                                onClick = { (onDismiss ?: onDismissRequest)() }
                            )
                        }
                        if (confirmText != null) {
                            DialogButton(
                                label = confirmText,
                                filled = true,
                                destructive = destructive,
                                modifier = Modifier.weight(1f),
                                onClick = { (onConfirm ?: onDismissRequest)() }
                            )
                        }
                    }
                }
            }
        }
    }
}

@Composable
private fun DialogButton(
    label: String,
    filled: Boolean,
    modifier: Modifier = Modifier,
    destructive: Boolean = false,
    onClick: () -> Unit
) {
    val accent = if (destructive) MaterialTheme.colorScheme.error else MaterialTheme.colorScheme.primary
    val shape = RoundedCornerShape(14.dp)
    Box(
        modifier = modifier
            .height(46.dp)
            .clip(shape)
            .then(
                if (filled) Modifier.background(accent)
                else Modifier.border(1.dp, MaterialTheme.colorScheme.outlineVariant.copy(alpha = 0.6f), shape)
            )
            .clickable { onClick() },
        contentAlignment = Alignment.Center
    ) {
        Text(
            text = label,
            style = MaterialTheme.typography.titleSmall,
            fontWeight = FontWeight.Bold,
            textAlign = TextAlign.Center,
            color = if (filled) MaterialTheme.colorScheme.onPrimary else MaterialTheme.colorScheme.onSurface
        )
    }
}
