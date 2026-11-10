use serde::Deserialize;

use super::clean_text;

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
pub struct SearchSuggestions {
    #[serde(default, rename = "tag")]
    pub items: Vec<SearchSuggestion>,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(from = "RawSuggestion")]
pub struct SearchSuggestion {
    pub value: String,
    pub segments: Vec<SuggestionSegment>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SuggestionSegment {
    pub text: String,
    pub highlight: bool,
}

#[derive(Deserialize)]
struct RawSuggestion {
    #[serde(default)]
    value: String,
    #[serde(default)]
    name: String,
}

impl From<RawSuggestion> for SearchSuggestion {
    fn from(raw: RawSuggestion) -> Self {
        let mut segments = parse_segments(&raw.name);
        let value = if raw.value.trim().is_empty() {
            segments
                .iter()
                .map(|segment| segment.text.as_str())
                .collect()
        } else {
            raw.value
        };
        if segments.is_empty() {
            segments.push(SuggestionSegment {
                text: value.clone(),
                highlight: false,
            });
        }

        Self { value, segments }
    }
}

fn parse_segments(html: &str) -> Vec<SuggestionSegment> {
    let mut segments = Vec::new();
    let mut rest = html;
    let mut highlighted = false;

    loop {
        if highlighted {
            match rest.find("</em>") {
                Some(index) => {
                    push_segment(&mut segments, &rest[..index], true);
                    rest = &rest[index + "</em>".len()..];
                    highlighted = false;
                }
                None => {
                    push_segment(&mut segments, rest, true);
                    break;
                }
            }
        } else {
            match rest.find("<em") {
                Some(index) => {
                    push_segment(&mut segments, &rest[..index], false);
                    let tag = &rest[index..];
                    let Some(end) = tag.find('>') else {
                        break;
                    };
                    rest = &tag[end + 1..];
                    highlighted = true;
                }
                None => {
                    push_segment(&mut segments, rest, false);
                    break;
                }
            }
        }
    }

    segments
}

fn push_segment(segments: &mut Vec<SuggestionSegment>, text: &str, highlight: bool) {
    let text = clean_text(text);
    if text.is_empty() {
        return;
    }

    match segments.last_mut() {
        Some(last) if last.highlight == highlight => last.text.push_str(&text),
        _ => segments.push(SuggestionSegment { text, highlight }),
    }
}

#[cfg(test)]
mod tests {
    use super::{SearchSuggestion, SearchSuggestions, SuggestionSegment};

    #[test]
    fn splits_highlighted_suggestion_names() {
        let suggestions: SearchSuggestions = serde_json::from_str(
            r#"{
                "tag": [
                    {
                        "value": "aaa颁奖典礼",
                        "name": "<em class=\"suggest_high_light\">aaa</em>颁奖典礼"
                    },
                    {
                        "value": "普通建议",
                        "name": "普通建议"
                    }
                ]
            }"#,
        )
        .expect("suggestions");

        let first = &suggestions.items[0];
        assert_eq!(first.value, "aaa颁奖典礼");
        assert_eq!(
            first.segments,
            [
                SuggestionSegment {
                    text: "aaa".into(),
                    highlight: true
                },
                SuggestionSegment {
                    text: "颁奖典礼".into(),
                    highlight: false
                }
            ]
        );

        let second = &suggestions.items[1];
        assert_eq!(second.value, "普通建议");
        assert_eq!(
            second.segments,
            [SuggestionSegment {
                text: "普通建议".into(),
                highlight: false
            }]
        );
    }

    #[test]
    fn falls_back_to_the_value_when_the_name_is_missing() {
        let suggestion: SearchSuggestion =
            serde_json::from_str(r#"{"value":"没有高亮"}"#).expect("suggestion");

        assert_eq!(suggestion.value, "没有高亮");
        assert_eq!(
            suggestion.segments,
            [SuggestionSegment {
                text: "没有高亮".into(),
                highlight: false
            }]
        );
    }
}
