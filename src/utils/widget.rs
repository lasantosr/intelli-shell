use ratatui::{style::Style, text::Span};
use unicode_width::UnicodeWidthChar;

/// Truncates a slice of spans to fit within a maximum width
pub fn truncate_spans<'a>(spans: &[Span<'a>], max_width: u16) -> (Vec<Span<'a>>, u16) {
    let mut current_width: u16 = 0;
    let mut truncated_spans: Vec<Span<'a>> = Vec::new();

    for span in spans {
        let span_width = span.width() as u16;
        if current_width.saturating_add(span_width) <= max_width {
            current_width = current_width.saturating_add(span_width);
            truncated_spans.push(span.clone());
        } else {
            let remaining_width = max_width.saturating_sub(current_width);
            if remaining_width > 0 {
                let mut content = String::new();
                let mut content_width: u16 = 0;
                for c in span.content.as_ref().chars() {
                    let char_width = UnicodeWidthChar::width(c).unwrap_or(0) as u16;
                    if content_width.saturating_add(char_width) <= remaining_width {
                        content.push(c);
                        content_width = content_width.saturating_add(char_width);
                    } else {
                        break;
                    }
                }
                if !content.is_empty() {
                    truncated_spans.push(Span::styled(content, span.style));
                    current_width = current_width.saturating_add(content_width);
                }
            }
            break;
        }
    }
    (truncated_spans, current_width)
}

/// Truncates a slice of spans and adds an ellipsis if truncation occurred
pub fn truncate_spans_with_ellipsis<'a>(spans: &[Span<'a>], max_width: u16) -> (Vec<Span<'a>>, u16) {
    let original_width = spans.iter().map(|s| s.width()).sum::<usize>() as u16;
    if original_width <= max_width {
        return (spans.to_vec(), original_width);
    }
    if max_width == 0 {
        return (Vec::new(), 0);
    }

    // Reserve space for ellipsis
    let target_width = max_width.saturating_sub(1);
    let (mut truncated_spans, mut current_width) = truncate_spans(spans, target_width);

    // Get style from the last span for the ellipsis, or default
    let ellipsis_style = truncated_spans
        .last()
        .map_or_else(|| spans.first().map_or(Style::default(), |s| s.style), |s| s.style);

    truncated_spans.push(Span::styled("…", ellipsis_style));
    current_width += 1;

    (truncated_spans, current_width)
}
