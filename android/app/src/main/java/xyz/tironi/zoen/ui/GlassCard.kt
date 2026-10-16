package xyz.tironi.zoen.ui

import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Card
import androidx.compose.material3.CardColors
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.CardElevation
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Shape
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.unit.dp

private val CardShape = RoundedCornerShape(22.dp)

/** Shared iOS-style glass surface with Material card semantics. */
@Composable
fun GlassCard(
    modifier: Modifier = Modifier,
    shape: Shape = CardShape,
    colors: CardColors? = null,
    elevation: CardElevation? = null,
    border: BorderStroke? = null,
    content: @Composable ColumnScope.() -> Unit,
) = Card(
    modifier = modifier.zoenGlass(shape),
    shape = shape,
    colors = colors ?: CardDefaults.cardColors(containerColor = Color.Transparent),
    elevation = elevation ?: CardDefaults.cardElevation(),
    border = border,
    content = content,
)

/** Clickable cards use Material3's interactions, indication and disabled styling. */
@Composable
fun GlassCard(
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
    enabled: Boolean = true,
    shape: Shape = CardShape,
    colors: CardColors? = null,
    elevation: CardElevation? = null,
    border: BorderStroke? = null,
    interactionSource: MutableInteractionSource? = null,
    content: @Composable ColumnScope.() -> Unit,
) = Card(
    onClick = onClick,
    modifier = modifier.zoenGlass(shape),
    enabled = enabled,
    shape = shape,
    colors = colors ?: CardDefaults.cardColors(containerColor = Color.Transparent),
    elevation = elevation ?: CardDefaults.cardElevation(),
    border = border,
    interactionSource = interactionSource,
    content = content,
)
