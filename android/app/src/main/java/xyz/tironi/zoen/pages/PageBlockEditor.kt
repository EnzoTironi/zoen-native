package xyz.tironi.zoen.pages

import androidx.compose.foundation.layout.*
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.rounded.*
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.focus.onFocusChanged
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.input.key.*
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.*
import androidx.compose.ui.text.font.*
import androidx.compose.ui.text.input.*
import androidx.compose.ui.text.style.TextDecoration
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import xyz.tironi.zoen.R
import xyz.tironi.zoen.core.PageBlockDto

fun styledPageText(block: PageBlockDto, linkColor: Color, codeColor: Color): AnnotatedString = buildAnnotatedString {
    append(block.text.replace('\u2028', '\n'))
    block.spans.forEach { span ->
        val start = span.start.toInt().coerceIn(0, length); val end = span.end.toInt().coerceIn(start, length)
        if (start < end) addStyle(when (span.key) {
            "b" -> SpanStyle(fontWeight = FontWeight.Bold)
            "i" -> SpanStyle(fontStyle = FontStyle.Italic)
            "s" -> SpanStyle(textDecoration = TextDecoration.LineThrough)
            "c" -> SpanStyle(fontFamily = FontFamily.Monospace, background = codeColor)
            "a" -> SpanStyle(color = linkColor, textDecoration = TextDecoration.Underline)
            else -> SpanStyle()
        }, start, end)
    }
}

@Composable
fun pageTextStyle(block: PageBlockDto): TextStyle = when (block.kind) {
        "heading" -> MaterialTheme.typography.headlineMedium.copy(
            fontSize = when (block.level) { 1u -> 30.sp; 2u -> 23.sp; 3u -> 19.sp; else -> 17.sp },
            lineHeight = when (block.level) { 1u -> 36.sp; 2u -> 29.sp; 3u -> 25.sp; else -> 23.sp },
            fontWeight = if (block.level <= 2u) FontWeight.Bold else FontWeight.SemiBold,
        )
        "code", "raw" -> MaterialTheme.typography.bodyMedium.copy(fontFamily = FontFamily.Monospace, fontSize = if (block.kind == "raw") 14.sp else 15.sp)
        "quote" -> MaterialTheme.typography.bodyLarge.copy(fontSize = 17.sp, fontStyle = FontStyle.Italic)
        else -> MaterialTheme.typography.bodyLarge.copy(fontSize = 17.sp, textDecoration = if (block.kind == "task" && block.checked) TextDecoration.LineThrough else null)
    }.copy(color = if (block.kind in listOf("quote", "raw") || block.kind == "task" && block.checked) MaterialTheme.colorScheme.onSurfaceVariant else MaterialTheme.colorScheme.onSurface)

@Composable
fun PageBlockEditor(block: PageBlockDto, focus: Boolean, changed: (PageBlockDto) -> Unit, enter: (PageBlockDto, Int) -> Unit, selection: (TextRange, Boolean) -> Unit, format: () -> Unit, modifier: Modifier = Modifier, selectedRange: TextRange? = null, typingMarks: Map<String, String?> = emptyMap(), editable: Boolean = true, composing: (Boolean) -> Unit = {}) {
    var value by remember(block.id) { mutableStateOf(TextFieldValue(block.text.replace('\u2028', '\n'))) }
    var composingNow by remember(block.id) { mutableStateOf(false) }
    val reportComposition by rememberUpdatedState(composing)
    fun compositionChanged(active: Boolean) {
        if (active != composingNow) { composingNow = active; reportComposition(active) }
    }
    DisposableEffect(block.id) { onDispose { if (composingNow) reportComposition(false) } }
    LaunchedEffect(editable) { if (!editable && composingNow) { value = value.copy(composition = null); compositionChanged(false) } }
    val requester = remember { FocusRequester() }
    LaunchedEffect(block.text, value.composition) {
        val text = block.text.replace('\u2028', '\n')
        if (value.text != text && value.composition == null) {
            val nextRange = TextRange(PageTextOffsets.remap(value.selection.start, value.text, text), PageTextOffsets.remap(value.selection.end, value.text, text))
            value = value.copy(text = text, selection = nextRange)
            if (focus) selection(nextRange, false)
        }
    }
    LaunchedEffect(selectedRange) {
        if (value.composition == null) selectedRange?.let { value = value.copy(selection = TextRange(PageTextOffsets.boundary(value.text, it.start), PageTextOffsets.boundary(value.text, it.end))) }
    }
    LaunchedEffect(focus) { if (focus && block.kind != "divider" && block.kind != "image") requester.requestFocus() }
    val style = pageTextStyle(block)
    val linkColor = MaterialTheme.colorScheme.primary
    val codeColor = MaterialTheme.colorScheme.surfaceContainerHighest
    val transformation = remember(block, linkColor, codeColor) {
        VisualTransformation { text -> TransformedText(styledPageText(block.copy(text = text.text.replace('\n', '\u2028')), linkColor, codeColor), OffsetMapping.Identity) }
    }
    Row(modifier.fillMaxWidth().padding(start = (block.indent.coerceAtMost(8u).toInt() * 12).dp), verticalAlignment = Alignment.Top) {
        when (block.kind) {
            "task" -> Checkbox(block.checked, { changed(block.copy(checked = it)) }, enabled = editable)
            "bullet" -> Text("•", Modifier.padding(top = 16.dp, end = 8.dp))
            "numbered" -> Text("${block.number.coerceAtLeast(1u)}.", Modifier.padding(top = 16.dp, end = 8.dp))
            "quote" -> VerticalDivider(Modifier.padding(top = 8.dp, end = 10.dp).height(48.dp), thickness = 3.dp)
        }
        Column(Modifier.weight(1f)) {
            when (block.kind) {
                "divider" -> HorizontalDivider(Modifier.padding(vertical = 24.dp))
                "image" -> {
                    Row(verticalAlignment = Alignment.CenterVertically) { Icon(Icons.Rounded.Image, null); Text(block.alt, Modifier.padding(start = 8.dp)) }
                    if (focus && editable) {
                        OutlinedTextField(block.url, { changed(block.copy(url = it)) }, label = { Text(stringResource(R.string.page_image_url)) }, modifier = Modifier.fillMaxWidth(), singleLine = true)
                        OutlinedTextField(block.alt, { changed(block.copy(alt = it)) }, label = { Text(stringResource(R.string.page_image_alt)) }, modifier = Modifier.fillMaxWidth(), singleLine = true)
                    }
                }
                else -> {
                    BasicTextField(value, { next ->
                        if (!editable) return@BasicTextField
                        compositionChanged(next.composition != null)
                        val textChanged = next.text != value.text
                        val marks = PageEditing.marksAt(block, value.selection.start) + typingMarks
                        val inserted = next.composition == null && next.text.length > value.text.length && next.selection.collapsed && next.selection.start > 0 && next.text.getOrNull(next.selection.start - 1) == '\n'
                        if (inserted && block.kind !in listOf("code", "raw", "quote")) {
                            val offset = next.selection.start - 1
                            if (block.text.isEmpty() && block.kind in listOf("task", "bullet", "numbered")) changed(block.copy(kind = "paragraph", indent = 0u))
                            else enter(PageEditing.replaceText(block, next.text.removeRange(offset, offset + 1).replace('\n', '\u2028'), marks), offset)
                        } else {
                            if (textChanged) {
                                val updated = PageEditing.replaceText(block, next.text.replace('\n', '\u2028'), marks)
                                val formatted = if (next.composition == null) PageEditing.shortcut(updated) else updated
                                changed(formatted)
                                value = if (formatted.text != updated.text) next.copy(text = formatted.text.replace('\u2028', '\n'), selection = TextRange.Zero) else next
                            } else value = next
                            selection(value.selection, textChanged)
                        }
                    }, modifier = Modifier.fillMaxWidth().padding(vertical = 12.dp).testTag("page-block:${block.id}").focusRequester(requester).onPreviewKeyEvent { event ->
                        if (editable && event.key == Key.Enter && event.isShiftPressed && event.type == KeyEventType.KeyDown) {
                            val next = PageEditing.hardBreak(block, value.selection.start, value.selection.end)
                            val caret = TextRange(minOf(value.selection.start, value.selection.end) + 1)
                            value = TextFieldValue(next.text.replace('\u2028', '\n'), caret)
                            changed(next); selection(caret, true); true
                        } else false
                    }.onFocusChanged {
                        if (it.isFocused) selection(value.selection, false)
                        else if (composingNow) { value = value.copy(composition = null); compositionChanged(false) }
                    }, textStyle = style,
                        readOnly = !editable, cursorBrush = SolidColor(MaterialTheme.colorScheme.primary),
                        visualTransformation = transformation, keyboardOptions = KeyboardOptions(capitalization = if (block.kind in listOf("code", "raw")) KeyboardCapitalization.None else KeyboardCapitalization.Sentences),
                        decorationBox = { field -> Box { if (value.text.isEmpty()) Text(stringResource(R.string.block_text), style = style, color = MaterialTheme.colorScheme.onSurfaceVariant); field() } })
                    if (block.kind == "code" && focus && editable) OutlinedTextField(block.lang, { changed(block.copy(lang = it.take(32))) }, label = { Text(stringResource(R.string.page_code_language)) }, modifier = Modifier.fillMaxWidth(), singleLine = true)
                }
            }
        }
        if (editable) IconButton(onClick = format) { Icon(Icons.Rounded.MoreVert, stringResource(R.string.format)) }
    }
}
