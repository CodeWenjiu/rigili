use rigili_bilibili::{CommentContent, CommentEmote, is_emote_image};

pub fn message_markdown(content: &CommentContent) -> String {
    let mut markdown = String::with_capacity(content.message.len() + 40);
    let Some(emotes) = content.emote.as_ref() else {
        push_message_text(&mut markdown, &content.message);
        return markdown;
    };

    let mut rest = content.message.as_str();
    while !rest.is_empty() {
        let mut next: Option<(usize, &str, &CommentEmote)> = None;
        for (token, emote) in emotes {
            if token.is_empty() || !is_emote_image(&emote.url) {
                continue;
            }
            let Some(index) = rest.find(token) else {
                continue;
            };
            if next.is_none_or(|(best, best_token, _)| {
                index < best || (index == best && token.len() > best_token.len())
            }) {
                next = Some((index, token, emote));
            }
        }

        let Some((index, token, emote)) = next else {
            push_message_text(&mut markdown, rest);
            break;
        };
        push_message_text(&mut markdown, &rest[..index]);
        let url = if emote.url.starts_with("//") {
            format!("https:{}", emote.url)
        } else {
            emote.url.clone()
        };
        markdown.push_str("<img src=\"");
        markdown.push_str(&escape_attr(&url));
        markdown.push_str("\" width=\"20\" height=\"20\" alt=\"");
        markdown.push_str(&escape_attr(token));
        markdown.push_str("\" />");
        rest = &rest[index + token.len()..];
    }
    markdown
}

fn push_message_text(markdown: &mut String, text: &str) {
    for ch in text.chars() {
        match ch {
            '\n' => markdown.push_str("  \n"),
            '\r' => {}
            c if c.is_ascii_punctuation() => {
                markdown.push('\\');
                markdown.push(c);
            }
            c => markdown.push(c),
        }
    }
}

fn escape_attr(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for ch in value.chars() {
        match ch {
            '&' => escaped.push_str("&amp;"),
            '"' => escaped.push_str("&quot;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            _ => escaped.push(ch),
        }
    }
    escaped
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use rigili_bilibili::{CommentContent, CommentEmote};

    use super::message_markdown;

    #[test]
    fn keeps_text_emotes_as_text() {
        let content = CommentContent {
            message: "(╯°□°）╯︵ ┻━┻".into(),
            emote: Some(HashMap::from([(
                "(╯°□°）╯︵ ┻━┻".into(),
                CommentEmote {
                    text: "(╯°□°）╯︵ ┻━┻".into(),
                    url: "(╯°□°）╯︵ ┻━┻".into(),
                },
            )])),
            pictures: Vec::new(),
        };

        let markdown = message_markdown(&content);
        assert!(!markdown.contains("<img"));
        assert!(markdown.contains("╯°□°"));
    }

    #[test]
    fn renders_emotes_and_escapes_message() {
        let content = CommentContent {
            message: "a [doge] <b>".into(),
            emote: Some(HashMap::from([(
                "[doge]".into(),
                CommentEmote {
                    text: "[doge]".into(),
                    url: "//i0.hdslb.com/doge.png".into(),
                },
            )])),
            pictures: Vec::new(),
        };

        assert_eq!(
            message_markdown(&content),
            "a <img src=\"https://i0.hdslb.com/doge.png\" width=\"20\" height=\"20\" alt=\"[doge]\" /> \\<b\\>"
        );
    }
}
