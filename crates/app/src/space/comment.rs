use std::collections::HashSet;

use gpui_kit::assets::IconName;
use gpui_kit::component::avatar::Avatar;
use gpui_kit::component::button::*;
use gpui_kit::component::spinner::Spinner;
use gpui_kit::component::text::TextView;
use gpui_kit::component::*;
use gpui_kit::prelude::*;
use gpui_kit::*;
use rigili_bilibili::api::comment::CommentSort;
use rigili_bilibili::{
    Comment, CommentContent, CommentEmote, CommentPage, DynamicArchive, DynamicItem, thumbnail,
};
use rigili_core::backend::{self, Backend};
use rigili_core::runtime::spawn_api;
use rigili_core::{api_error_message, api_task_error_message, format_count, format_date};

use crate::grid::append_unique_by;
use crate::platform::image_cache;

pub(super) fn open(dynamic: DynamicItem, window: &mut Window, cx: &mut App) {
    let view = cx.new(|_| CommentsView::new(dynamic));
    view.update(cx, |view, cx| view.load(true, cx));
    window.open_sheet(cx, move |sheet, _, cx| {
        sheet.title("评论").size(px(480.)).resizable(false).child(
            div()
                .size_full()
                .image_cache(image_cache::get(cx))
                .child(view.clone()),
        )
    });
}

struct CommentsView {
    backend: Option<Backend>,
    dynamic: DynamicItem,
    sort: CommentSort,
    items: Vec<Comment>,
    seen: HashSet<String>,
    page: u32,
    total: u64,
    has_more: bool,
    loading: bool,
    loading_more: bool,
    loaded: bool,
    error: Option<String>,
    more_error: Option<String>,
    generation: u64,
}

impl CommentsView {
    fn new(dynamic: DynamicItem) -> Self {
        Self {
            backend: None,
            dynamic,
            sort: CommentSort::default(),
            items: Vec::new(),
            seen: HashSet::new(),
            page: 0,
            total: 0,
            has_more: true,
            loading: false,
            loading_more: false,
            loaded: false,
            error: None,
            more_error: None,
            generation: 0,
        }
    }

    fn backend(&mut self, cx: &mut Context<Self>) -> Result<Backend, String> {
        if let Some(backend) = &self.backend {
            return Ok(backend.clone());
        }
        match backend::get(cx) {
            Ok(backend) => {
                self.backend = Some(backend.clone());
                Ok(backend)
            }
            Err(error) => Err(api_error_message(error)),
        }
    }

    fn load(&mut self, reset: bool, cx: &mut Context<Self>) {
        if !reset && (self.loading || self.loading_more || !self.has_more) {
            return;
        }
        let (kind, oid) = self.dynamic.comment_target();
        let oid = oid.to_owned();
        let page = if reset { 1 } else { self.page + 1 };
        let sort = self.sort;
        let backend = match self.backend(cx) {
            Ok(backend) => backend,
            Err(error) => {
                if reset {
                    self.error = Some(error);
                } else {
                    self.more_error = Some(error);
                }
                cx.notify();
                return;
            }
        };
        let generation = if reset {
            self.generation += 1;
            self.items.clear();
            self.seen.clear();
            self.page = 0;
            self.total = 0;
            self.has_more = true;
            self.loading = true;
            self.loading_more = false;
            self.loaded = false;
            self.error = None;
            self.more_error = None;
            self.generation
        } else {
            self.loading_more = true;
            self.more_error = None;
            self.generation
        };
        cx.notify();

        spawn_api(
            cx,
            async move { backend.comments(kind, &oid, page, sort).await },
            move |this, result, cx| {
                if this.generation != generation {
                    return;
                }
                this.loading = false;
                this.loading_more = false;
                match result {
                    Ok(page_data) => {
                        let CommentPage {
                            page: info,
                            replies,
                        } = page_data;
                        let incoming = replies.unwrap_or_default();
                        let incoming_empty = incoming.is_empty();
                        append_unique_by(&mut this.items, &mut this.seen, incoming, |comment| {
                            comment.rpid.to_string()
                        });
                        this.page = if info.num == 0 { page } else { info.num };
                        this.total = info.count.max(info.acount);
                        this.has_more = !incoming_empty && (this.items.len() as u64) < this.total;
                        this.loaded = true;
                        this.error = None;
                        this.more_error = None;
                    }
                    Err(error) => {
                        let message = api_task_error_message(error, cx);
                        if reset {
                            this.error = Some(message);
                        } else {
                            this.more_error = Some(message);
                        }
                    }
                }
                cx.notify();
            },
        );
    }

    fn set_sort(&mut self, sort: CommentSort, cx: &mut Context<Self>) {
        if self.sort == sort {
            return;
        }
        self.sort = sort;
        self.load(true, cx);
    }

    fn render_summary(&self, cx: &App) -> AnyElement {
        let primary = cx.theme().primary;
        let muted = cx.theme().muted_foreground;
        let border = cx.theme().border;
        let radius = cx.theme().radius;
        let author = self.dynamic.author();
        let text = self.dynamic.text();
        let tip = self.dynamic.forward_tip();
        let archive = self.dynamic.archive();

        v_flex()
            .gap_3()
            .p_3()
            .rounded(radius)
            .bg(cx.theme().muted.opacity(0.25))
            .child(
                h_flex()
                    .items_center()
                    .gap_3()
                    .child(
                        Avatar::new()
                            .src(author.face.clone())
                            .name(author.name.clone())
                            .size(px(36.)),
                    )
                    .child(
                        v_flex()
                            .flex_1()
                            .min_w_0()
                            .gap_1()
                            .child(
                                div()
                                    .text_sm()
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(primary)
                                    .child(author.name.clone()),
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(muted)
                                    .child(author.pub_time.clone()),
                            ),
                    ),
            )
            .when(!text.is_empty(), |this| {
                this.child(
                    div()
                        .text_sm()
                        .line_height(relative(1.6))
                        .child(text.to_owned()),
                )
            })
            .when_some(archive, |this, archive| {
                this.child(archive_summary(archive, cx))
            })
            .when_some(tip, |this, tip| {
                this.child(
                    h_flex()
                        .items_center()
                        .gap_2()
                        .p_2()
                        .rounded(radius)
                        .border_1()
                        .border_color(border)
                        .text_xs()
                        .text_color(muted)
                        .child(Icon::new(IconName::CircleAlert).with_size(px(14.)))
                        .child(tip.to_owned()),
                )
            })
            .into_any_element()
    }

    fn render_sort_tabs(&self, cx: &mut Context<Self>) -> AnyElement {
        let primary = cx.theme().primary;
        let muted = cx.theme().muted_foreground;

        h_flex()
            .items_center()
            .gap_4()
            .children(
                [
                    (CommentSort::Hot, "最热", 0usize),
                    (CommentSort::Latest, "最新", 1usize),
                ]
                .map(|(sort, label, index)| {
                    let active = self.sort == sort;
                    let group = SharedString::from(format!("comment-sort-{index}"));
                    Button::new(("comment-sort", index))
                        .text()
                        .small()
                        .px_1()
                        .accessibility_label(label)
                        .on_click(cx.listener(move |this, _, _, cx| this.set_sort(sort, cx)))
                        .child(
                            div()
                                .group(group.clone())
                                .text_sm()
                                .text_color(if active { primary } else { muted })
                                .group_hover(group, move |this| this.text_color(primary))
                                .child(label),
                        )
                }),
            )
            .into_any_element()
    }

    fn render_comment(&self, comment: &Comment, depth: usize, cx: &App) -> AnyElement {
        let primary = cx.theme().primary;
        let foreground = cx.theme().foreground;
        let muted = cx.theme().muted_foreground;
        let member = &comment.member;
        let level = member.level_info.current_level;
        let time = if comment.reply_control.time_desc.is_empty() {
            format_date(comment.ctime)
        } else {
            comment.reply_control.time_desc.clone()
        };
        let nested = comment.replies();
        let nested_views = nested
            .iter()
            .take(2)
            .map(|reply| self.render_comment(reply, depth + 1, cx))
            .collect::<Vec<_>>();
        let avatar_size = if depth == 0 { 32. } else { 24. };
        let has_emotes = comment
            .content
            .emote
            .as_ref()
            .is_some_and(|emotes| !emotes.is_empty());
        let message = if has_emotes {
            TextView::markdown(
                ("comment-message", comment.rpid),
                message_markdown(&comment.content),
            )
            .text_sm()
            .line_height(relative(1.5))
            .text_color(foreground)
            .selectable(false)
            .into_any_element()
        } else {
            div()
                .text_sm()
                .line_height(relative(1.5))
                .child(comment.content.message.clone())
                .into_any_element()
        };

        v_flex()
            .gap_2()
            .child(
                h_flex()
                    .items_start()
                    .gap_3()
                    .child(
                        Avatar::new()
                            .src(member.avatar_url())
                            .name(member.uname.clone())
                            .size(px(avatar_size)),
                    )
                    .child(
                        v_flex()
                            .flex_1()
                            .min_w_0()
                            .gap_1()
                            .child(
                                h_flex()
                                    .items_center()
                                    .gap_2()
                                    .child(
                                        div()
                                            .text_sm()
                                            .text_color(primary)
                                            .child(member.uname.clone()),
                                    )
                                    .when(level > 0, |this| {
                                        this.child(
                                            div()
                                                .px_1()
                                                .rounded(px(3.))
                                                .bg(primary.opacity(0.12))
                                                .text_color(primary)
                                                .text_xs()
                                                .child(format!("LV{level}")),
                                        )
                                    })
                                    .child(div().text_xs().text_color(muted).child(time)),
                            )
                            .child(message)
                            .child(
                                h_flex()
                                    .items_center()
                                    .gap_3()
                                    .text_xs()
                                    .text_color(muted)
                                    .child(
                                        h_flex()
                                            .items_center()
                                            .gap_1()
                                            .child(Icon::new(IconName::ThumbsUp).with_size(px(12.)))
                                            .child(format_count(comment.like)),
                                    )
                                    .when(comment.rcount > 0, |this| {
                                        this.child(format!(
                                            "{} 条回复",
                                            format_count(comment.rcount)
                                        ))
                                    }),
                            ),
                    ),
            )
            .when(!nested_views.is_empty(), |this| {
                this.child(v_flex().gap_2().pl(px(44.)).children(nested_views))
            })
            .into_any_element()
    }

    fn render_initial_state(&self, cx: &mut Context<Self>) -> AnyElement {
        let muted = cx.theme().muted_foreground;
        let danger = cx.theme().danger;

        if self.loading && self.items.is_empty() {
            return h_flex()
                .py_6()
                .justify_center()
                .child(Spinner::new())
                .into_any_element();
        }
        if let Some(error) = self.error.clone() {
            return h_flex()
                .items_center()
                .gap_3()
                .child(div().text_sm().text_color(danger).child(error))
                .child(
                    Button::new("comment-retry")
                        .small()
                        .label("重试")
                        .on_click(cx.listener(|this, _, _, cx| this.load(true, cx))),
                )
                .into_any_element();
        }
        if self.loaded && self.items.is_empty() {
            return h_flex()
                .py_6()
                .justify_center()
                .text_sm()
                .text_color(muted)
                .child("暂无评论")
                .into_any_element();
        }
        div().into_any_element()
    }

    fn render_footer(&self, cx: &mut Context<Self>) -> AnyElement {
        let muted = cx.theme().muted_foreground;
        let danger = cx.theme().danger;

        if self.loading_more {
            return h_flex()
                .py_2()
                .justify_center()
                .child(Spinner::new().small())
                .into_any_element();
        }
        if let Some(error) = self.more_error.clone() {
            return h_flex()
                .items_center()
                .justify_center()
                .gap_3()
                .child(div().text_sm().text_color(danger).child(error))
                .child(
                    Button::new("comment-more-retry")
                        .small()
                        .label("重试")
                        .on_click(cx.listener(|this, _, _, cx| this.load(false, cx))),
                )
                .into_any_element();
        }
        if self.has_more {
            return h_flex()
                .justify_center()
                .child(
                    Button::new("comment-more")
                        .small()
                        .ghost()
                        .label("加载更多")
                        .on_click(cx.listener(|this, _, _, cx| this.load(false, cx))),
                )
                .into_any_element();
        }
        if self.loaded {
            return h_flex()
                .py_2()
                .justify_center()
                .text_xs()
                .text_color(muted)
                .child("没有更多评论")
                .into_any_element();
        }
        div().into_any_element()
    }
}

fn message_markdown(content: &CommentContent) -> String {
    let mut markdown = String::with_capacity(content.message.len() + 40);
    let Some(emotes) = content.emote.as_ref() else {
        push_message_text(&mut markdown, &content.message);
        return markdown;
    };

    let mut rest = content.message.as_str();
    while !rest.is_empty() {
        let mut next: Option<(usize, &str, &CommentEmote)> = None;
        for (token, emote) in emotes {
            if token.is_empty() || emote.url.is_empty() {
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

fn archive_summary(archive: &DynamicArchive, cx: &App) -> AnyElement {
    let muted = cx.theme().muted_foreground;
    let border = cx.theme().border;
    let radius = cx.theme().radius;

    h_flex()
        .items_center()
        .gap_3()
        .p_2()
        .rounded(radius)
        .border_1()
        .border_color(border)
        .child(
            div()
                .w(px(160.))
                .aspect_ratio(16. / 9.)
                .rounded(radius)
                .overflow_hidden()
                .child(
                    img(thumbnail(&archive.cover, 320))
                        .size_full()
                        .object_fit(ObjectFit::Cover),
                ),
        )
        .child(
            v_flex()
                .flex_1()
                .min_w_0()
                .gap_1()
                .child(
                    div()
                        .text_sm()
                        .line_height(relative(1.4))
                        .line_clamp(2)
                        .child(archive.title.clone()),
                )
                .child(
                    h_flex()
                        .items_center()
                        .gap_3()
                        .text_xs()
                        .text_color(muted)
                        .child(
                            h_flex()
                                .items_center()
                                .gap_1()
                                .child(Icon::new(IconName::Play).with_size(px(12.)))
                                .child(archive.stat.play.clone()),
                        )
                        .child(
                            h_flex()
                                .items_center()
                                .gap_1()
                                .child(Icon::new(IconName::MessageCircle).with_size(px(12.)))
                                .child(archive.stat.danmaku.clone()),
                        )
                        .when(!archive.duration_text.is_empty(), |this| {
                            this.child(archive.duration_text.clone())
                        }),
                ),
        )
        .into_any_element()
}

impl Render for CommentsView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let muted = cx.theme().muted_foreground;
        let border = cx.theme().border;
        let summary = self.render_summary(cx);
        let sort_tabs = self.render_sort_tabs(cx);
        let initial = self.render_initial_state(cx);
        let footer = self.render_footer(cx);
        let total = if self.loaded {
            Some(format!("共 {} 条评论", format_count(self.total)))
        } else {
            None
        };
        let comments = self
            .items
            .iter()
            .map(|comment| {
                div()
                    .border_b_1()
                    .border_color(border)
                    .pb_3()
                    .child(self.render_comment(comment, 0, cx))
            })
            .collect::<Vec<_>>();

        v_flex()
            .gap_4()
            .pb_4()
            .child(summary)
            .child(
                h_flex()
                    .items_center()
                    .gap_4()
                    .child(sort_tabs)
                    .child(div().flex_1())
                    .when_some(total, |this, total| {
                        this.child(div().text_xs().text_color(muted).child(total))
                    }),
            )
            .child(initial)
            .children(comments)
            .child(footer)
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use rigili_bilibili::{CommentContent, CommentEmote};

    use super::message_markdown;

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
        };

        assert_eq!(
            message_markdown(&content),
            "a <img src=\"https://i0.hdslb.com/doge.png\" width=\"20\" height=\"20\" alt=\"[doge]\" /> \\<b\\>"
        );
    }
}
