package com.lumen.ui.components

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Info
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import com.lumen.ui.screens.LocalStrings
import com.lumen.ui.screens.LumenStrings
import com.lumen.ui.screens.SubscriptionUiModel

/**
 * The provider half of a subscription card: the decoded announcement, the optional
 * banner and one button per link the panel actually sent.
 *
 * Every value arrives from the app layer already decoded, trimmed and validated, so
 * nothing here parses or sanitises. A field the panel never sent is null and its row
 * is not rendered at all: a provider that sends nothing renders nothing.
 */

/** One provider button: [label] comes from the strings table, [target] from the panel. */
data class ProviderLink(val key: String, val label: String, val target: String)

private fun String?.orNullIfBlank(): String? = this?.trim()?.takeIf { it.isNotEmpty() }

/**
 * Buttons for the links the provider sent, in the order the reference client shows
 * them. The email address is turned into a `mailto:` target here; the model carries
 * the bare address.
 */
fun providerLinks(sub: SubscriptionUiModel, s: LumenStrings): List<ProviderLink> = buildList {
    // The reference client leads with the provider's channel/bot.
    sub.telegramUrl.orNullIfBlank()?.let { add(ProviderLink("channel", s.providerChannel, it)) }
    sub.supportUrl.orNullIfBlank()?.let { add(ProviderLink("support", s.providerSupport, it)) }
    sub.supportEmail.orNullIfBlank()?.let { add(ProviderLink("email", s.providerEmail, "mailto:$it")) }
    sub.websiteUrl.orNullIfBlank()?.let { add(ProviderLink("website", s.providerWebsite, it)) }
    sub.premiumUrl.orNullIfBlank()?.let { add(ProviderLink("premium", s.providerPremium, it)) }
}

/** Announcement to lay out, or null when the provider sent none. Up to 200 chars, multi-line. */
fun subscriptionAnnouncement(sub: SubscriptionUiModel): String? = sub.announce.orNullIfBlank()

/** The panel can ask for the subscription URL to stay out of the UI (`hide-url`). */
fun subscriptionUrlVisible(sub: SubscriptionUiModel): Boolean =
    !sub.hideUrl && sub.url.isNotBlank()

/** True when the provider sent anything that belongs in the banner. */
fun hasProviderBanner(sub: SubscriptionUiModel): Boolean =
    sub.bannerText.orNullIfBlank() != null ||
        (sub.bannerButtonText.orNullIfBlank() != null && sub.bannerButtonUrl.orNullIfBlank() != null)

/**
 * True when [SubscriptionProviderCard] would draw anything for [sub]. Callers need this
 * before composing it, because a card that draws nothing still leaves the rounded bottom
 * corner of the tile to whatever sits above it.
 */
fun hasProviderCard(sub: SubscriptionUiModel): Boolean =
    subscriptionAnnouncement(sub) != null ||
        sub.description.orNullIfBlank() != null ||
        hasProviderBanner(sub) ||
        sub.telegramUrl.orNullIfBlank() != null ||
        sub.supportUrl.orNullIfBlank() != null ||
        sub.supportEmail.orNullIfBlank() != null ||
        sub.websiteUrl.orNullIfBlank() != null ||
        sub.premiumUrl.orNullIfBlank() != null

/**
 * `#RRGGBB` to an opaque ARGB int, null for anything else. The app layer validates the
 * format, so null here only means "the provider sent no colour" — the caller then uses
 * the theme's own colours instead of painting an unreadable tile.
 */
fun parseProviderColor(value: String?): Int? {
    val hex = value.orNullIfBlank()?.removePrefix("#") ?: return null
    val isHex = hex.length == 6 && hex.all { it in '0'..'9' || it in 'a'..'f' || it in 'A'..'F' }
    if (!isHex) return null
    return (0xFF000000L or hex.toLong(16)).toInt()
}

/** True when black text reads better on [argb] than white; keeps light banners legible. */
fun providerColorPrefersDarkText(argb: Int): Boolean {
    val r = (argb shr 16) and 0xFF
    val g = (argb shr 8) and 0xFF
    val b = argb and 0xFF
    return (0.299 * r + 0.587 * g + 0.114 * b) / 255.0 > 0.55
}

/**
 * Announcement, banner and provider buttons, drawn as a continuation of the traffic
 * bar above it. Renders nothing at all when the provider sent none of them.
 */
@OptIn(ExperimentalLayoutApi::class)
@Composable
fun SubscriptionProviderCard(
    sub: SubscriptionUiModel,
    onOpenUrl: (String) -> Unit,
    modifier: Modifier = Modifier
) {
    val s = LocalStrings.current
    val announcement = subscriptionAnnouncement(sub)
    val announcementUrl = sub.announceUrl.orNullIfBlank()
    val description = sub.description.orNullIfBlank()
    val links = providerLinks(sub, s)
    val banner = hasProviderBanner(sub)
    if (!hasProviderCard(sub)) return

    Column(
        modifier = modifier
            .fillMaxWidth()
            .background(MaterialTheme.colorScheme.surfaceVariant.copy(alpha = 0.85f))
            .padding(start = 14.dp, end = 14.dp, bottom = 12.dp)
    ) {
        if (announcement != null) {
            Row(
                modifier = Modifier
                    .fillMaxWidth()
                    .then(
                        if (announcementUrl != null) {
                            Modifier
                                .clip(RoundedCornerShape(10.dp))
                                .clickable { onOpenUrl(announcementUrl) }
                        } else {
                            Modifier
                        }
                    )
                    .padding(vertical = 4.dp),
                verticalAlignment = Alignment.Top
            ) {
                Icon(
                    imageVector = Icons.Filled.Info,
                    contentDescription = s.providerAnnouncement,
                    tint = MaterialTheme.colorScheme.primary,
                    modifier = Modifier.size(16.dp)
                )
                Spacer(Modifier.width(6.dp))
                // No maxLines: the announcement is up to 200 characters and keeps the
                // provider's own line breaks, so it wraps instead of being cut off.
                Text(
                    text = announcement,
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onSurface,
                    modifier = Modifier.weight(1f)
                )
            }
        }
        if (description != null) {
            Spacer(Modifier.height(2.dp))
            Text(
                text = description,
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant
            )
        }
        if (banner) {
            Spacer(Modifier.height(8.dp))
            ProviderBanner(sub = sub, onOpenUrl = onOpenUrl)
        }
        if (links.isNotEmpty()) {
            Spacer(Modifier.height(8.dp))
            FlowRow(
                modifier = Modifier.fillMaxWidth(),
                horizontalArrangement = Arrangement.spacedBy(8.dp),
                verticalArrangement = Arrangement.spacedBy(8.dp)
            ) {
                links.forEach { link ->
                    ProviderButton(
                        label = link.label,
                        container = MaterialTheme.colorScheme.primary.copy(alpha = 0.14f),
                        content = MaterialTheme.colorScheme.primary,
                        onClick = { onOpenUrl(link.target) }
                    )
                }
            }
        }
    }
}

/** Provider banner: its own background and button colours, with a theme fallback. */
@Composable
private fun ProviderBanner(sub: SubscriptionUiModel, onOpenUrl: (String) -> Unit) {
    val text = sub.bannerText?.trim()?.takeIf { it.isNotEmpty() }
    val buttonText = sub.bannerButtonText?.trim()?.takeIf { it.isNotEmpty() }
    val buttonUrl = sub.bannerButtonUrl?.trim()?.takeIf { it.isNotEmpty() }
    val bgArgb = parseProviderColor(sub.bannerBgColor)
    // Without a usable colour the banner stays on the theme's surface, never on a flat
    // black tile that would swallow its own text.
    val background = bgArgb?.let { Color(it) }
        ?: MaterialTheme.colorScheme.primary.copy(alpha = 0.12f)
    val foreground = bgArgb?.let { if (providerColorPrefersDarkText(it)) Color.Black else Color.White }
        ?: MaterialTheme.colorScheme.onSurface
    val buttonArgb = parseProviderColor(sub.bannerButtonColor)
    val buttonBackground = buttonArgb?.let { Color(it) } ?: MaterialTheme.colorScheme.primary
    val buttonForeground =
        buttonArgb?.let { if (providerColorPrefersDarkText(it)) Color.Black else Color.White }
            ?: MaterialTheme.colorScheme.onPrimary

    Column(
        modifier = Modifier
            .fillMaxWidth()
            .clip(RoundedCornerShape(14.dp))
            .background(background)
            .padding(12.dp)
    ) {
        if (text != null) {
            Text(
                text = text,
                style = MaterialTheme.typography.bodySmall,
                color = foreground
            )
        }
        if (buttonText != null && buttonUrl != null) {
            if (text != null) Spacer(Modifier.height(10.dp))
            ProviderButton(
                label = buttonText,
                container = buttonBackground,
                content = buttonForeground,
                onClick = { onOpenUrl(buttonUrl) }
            )
        }
    }
}

/** Pill button used by both the provider link row and the banner. */
@Composable
private fun ProviderButton(
    label: String,
    container: Color,
    content: Color,
    onClick: () -> Unit,
    modifier: Modifier = Modifier
) {
    val shape = RoundedCornerShape(12.dp)
    Row(
        modifier = modifier
            .clip(shape)
            .background(container)
            .border(1.dp, content.copy(alpha = 0.35f), shape)
            .clickable { onClick() }
            .padding(horizontal = 14.dp, vertical = 8.dp),
        verticalAlignment = Alignment.CenterVertically
    ) {
        // Labels wrap rather than truncate: Persian and Chinese buttons are wider
        // than the English ones and the provider's own banner label is free text.
        Text(
            text = label,
            style = MaterialTheme.typography.labelLarge,
            fontWeight = FontWeight.SemiBold,
            color = content
        )
    }
}

/**
 * Subscription properties: name, URL unless the provider hid it, and the same
 * provider links the card offers.
 */
@OptIn(ExperimentalLayoutApi::class)
@Composable
fun SubscriptionDetailsDialog(
    sub: SubscriptionUiModel,
    onDismiss: () -> Unit,
    onCopyUrl: (String) -> Unit,
    onOpenUrl: (String) -> Unit
) {
    val s = LocalStrings.current
    val links = providerLinks(sub, s)
    LumenDialog(
        title = s.subscriptionProperties,
        onDismissRequest = onDismiss,
        confirmText = s.done,
        onConfirm = onDismiss,
        content = {
            Column(modifier = Modifier.fillMaxWidth()) {
                Text(
                    text = sub.name,
                    style = MaterialTheme.typography.bodyLarge,
                    fontWeight = FontWeight.SemiBold,
                    color = MaterialTheme.colorScheme.onSurface
                )
                if (subscriptionUrlVisible(sub)) {
                    Spacer(Modifier.height(12.dp))
                    Text(
                        text = s.url,
                        style = MaterialTheme.typography.bodySmall,
                        color = MaterialTheme.colorScheme.onSurfaceVariant
                    )
                    Spacer(Modifier.height(4.dp))
                    Text(
                        text = sub.url,
                        style = MaterialTheme.typography.bodyMedium,
                        color = MaterialTheme.colorScheme.onSurface,
                        modifier = Modifier
                            .fillMaxWidth()
                            .clip(RoundedCornerShape(10.dp))
                            .clickable { onCopyUrl(sub.url) }
                    )
                }
                subscriptionAnnouncement(sub)?.let { announcement ->
                    Spacer(Modifier.height(12.dp))
                    Text(
                        text = s.providerAnnouncement,
                        style = MaterialTheme.typography.bodySmall,
                        color = MaterialTheme.colorScheme.onSurfaceVariant
                    )
                    Spacer(Modifier.height(4.dp))
                    Text(
                        text = announcement,
                        style = MaterialTheme.typography.bodyMedium,
                        color = MaterialTheme.colorScheme.onSurface
                    )
                }
                if (links.isNotEmpty()) {
                    Spacer(Modifier.height(12.dp))
                    FlowRow(
                        modifier = Modifier.fillMaxWidth(),
                        horizontalArrangement = Arrangement.spacedBy(8.dp),
                        verticalArrangement = Arrangement.spacedBy(8.dp)
                    ) {
                        links.forEach { link ->
                            ProviderButton(
                                label = link.label,
                                container = MaterialTheme.colorScheme.primary.copy(alpha = 0.14f),
                                content = MaterialTheme.colorScheme.primary,
                                onClick = { onOpenUrl(link.target) }
                            )
                        }
                    }
                }
            }
        }
    )
}
