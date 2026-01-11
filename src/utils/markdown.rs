use crossterm::style::{Attribute, Color, ContentStyle, Stylize};
use pulldown_cmark::{Event, HeadingLevel, Options, Parser, Tag, TagEnd};

use crate::config::Theme;

/// Renders a markdown string to an ANSI-styled string using the provided theme.
///
/// This implementation provides basic support for:
/// - Headers (styled based on theme comment style, no hashes, underlined for L3/L4)
/// - Emphasis (bold, italic)
/// - Lists (bullet points, task lists)
/// - Code (inline and blocks, styled based on theme accent and secondary styles)
/// - Paragraphs
pub fn render_markdown_to_ansi(markdown: &str, theme: &Theme) -> String {
    let parser = Parser::new_ext(markdown, Options::all());
    let mut output = String::new();
    let mut style_stack: Vec<ContentStyle> = vec![theme.primary];
    let mut list_depth: usize = 0;
    let mut code_block_buffer: Option<String> = None;

    fn ensure_newlines(output: &mut String, count: usize) {
        if output.is_empty() {
            return;
        }
        let current = if output.ends_with("\n\n") {
            2
        } else if output.ends_with('\n') {
            1
        } else {
            0
        };
        if current < count {
            output.push_str(&"\n".repeat(count - current));
        }
    }

    for event in parser {
        match event {
            Event::Start(tag) => match tag {
                Tag::Heading { level, .. } => {
                    ensure_newlines(&mut output, 2);
                    let mut style = theme.comment;
                    style.attributes.set(Attribute::Bold);
                    if matches!(level, HeadingLevel::H3 | HeadingLevel::H4) {
                        style.attributes.set(Attribute::Underlined);
                    }
                    style_stack.push(style);
                }
                Tag::Paragraph => {
                    // Only enforce newlines if the buffer ends in a newline (previous block ended)
                    // or is empty. If the buffer ends in text/whitespace (like a list bullet),
                    // we want this paragraph to continue on the same line.
                    if output.ends_with('\n') || output.is_empty() {
                        ensure_newlines(&mut output, 2);
                    }
                }
                Tag::Emphasis => {
                    let mut style = style_stack.last().cloned().unwrap_or_default();
                    style.attributes.set(Attribute::Italic);
                    style_stack.push(style);
                }
                Tag::Strong => {
                    let mut style = style_stack.last().cloned().unwrap_or_default();
                    style.attributes.set(Attribute::Bold);
                    style_stack.push(style);
                }
                Tag::List(_) => {
                    list_depth += 1;
                    if list_depth == 1 {
                        ensure_newlines(&mut output, 2);
                    } else {
                        ensure_newlines(&mut output, 1);
                    }
                }
                Tag::Item => {
                    ensure_newlines(&mut output, 1);
                    let indent = " ".repeat(list_depth.saturating_sub(1) * 2);
                    let bullet = match list_depth {
                        1 => "• ",
                        2 => "◦ ",
                        _ => "▪ ",
                    };
                    output.push_str("  "); // Base indentation
                    output.push_str(&indent);
                    output.push_str(bullet);
                }
                Tag::CodeBlock(kind) => {
                    ensure_newlines(&mut output, 2);
                    if let pulldown_cmark::CodeBlockKind::Fenced(lang) = kind
                        && !lang.is_empty()
                    {
                        let label_style = ContentStyle::default()
                            .with(Color::Black)
                            .on(Color::AnsiValue(244)) // Medium grey
                            .bold();
                        output.push_str(&label_style.apply(&format!(" {} ", lang)).to_string());
                        output.push('\n');
                    }
                    let mut style = theme.secondary;
                    if style.background_color.is_none() {
                        style.background_color = theme.highlight;
                    }
                    style_stack.push(style);
                    code_block_buffer = Some(String::new());
                }
                Tag::Link { .. } => {
                    let mut style = style_stack.last().cloned().unwrap_or_default();
                    style.attributes.set(Attribute::Underlined);
                    style.foreground_color = Some(Color::Blue);
                    style_stack.push(style);
                }
                Tag::Strikethrough => {
                    let mut style = style_stack.last().cloned().unwrap_or_default();
                    style.attributes.set(Attribute::CrossedOut);
                    style_stack.push(style);
                }
                _ => {}
            },
            Event::End(tag) => match tag {
                TagEnd::Heading { .. } => {
                    style_stack.pop();
                    ensure_newlines(&mut output, 1);
                }
                TagEnd::Paragraph => {
                    ensure_newlines(&mut output, 1);
                }
                TagEnd::Emphasis | TagEnd::Strong | TagEnd::Link | TagEnd::Strikethrough => {
                    style_stack.pop();
                }
                TagEnd::CodeBlock => {
                    let style = style_stack.pop().unwrap_or_default();
                    if let Some(code) = code_block_buffer.take() {
                        let lines: Vec<&str> = code.lines().collect();
                        let max_width = lines.iter().map(|l| l.len()).max().unwrap_or(0);
                        for line in lines {
                            let padded = format!("  {:<width$}  ", line, width = max_width);
                            output.push_str(&style.apply(&padded).to_string());
                            output.push('\n');
                        }
                    }
                    ensure_newlines(&mut output, 1);
                }
                TagEnd::List(_) => {
                    list_depth = list_depth.saturating_sub(1);
                    ensure_newlines(&mut output, 1);
                }
                TagEnd::Item => {
                    ensure_newlines(&mut output, 1);
                }
                _ => {}
            },
            Event::Text(text) => {
                if let Some(ref mut buffer) = code_block_buffer {
                    buffer.push_str(&text);
                } else {
                    let style = style_stack.last().cloned().unwrap_or_default();
                    output.push_str(&style.apply(&text).to_string());
                }
            }
            Event::Code(code) => {
                let mut style = theme.accent;
                if style.background_color.is_none() {
                    style.background_color = theme.highlight;
                }
                output.push_str(&style.apply(&code).to_string());
            }
            Event::TaskListMarker(checked) => {
                if checked {
                    output.push_str("[x] ");
                } else {
                    output.push_str("[ ] ");
                }
            }
            Event::SoftBreak | Event::HardBreak => {
                output.push('\n');
            }
            Event::Rule => {
                ensure_newlines(&mut output, 1);
                output.push_str(&theme.secondary.apply(&"─".repeat(60)).to_string());
                output.push('\n');
            }
            _ => {}
        }
    }

    output.trim_end().to_string()
}
