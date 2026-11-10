use std::sync::Arc;

use gpui_kit::assets::IconName;
use gpui_kit::component::avatar::Avatar;
use gpui_kit::component::list::ListItem;
use gpui_kit::component::scroll::Scrollbar;
use gpui_kit::component::*;
use gpui_kit::prelude::*;
use gpui_kit::*;
use rigili_bilibili::{VideoInfo, VideoPage};

use crate::format::{format_count, format_datetime, format_duration};

const INFO_WIDTH: Pixels = px(340.);

pub(super) enum InfoEvent {
    SelectPage(u64),
}

struct VideoDetails {
    owner: SharedString,
    avatar: SharedString,
    uid: SharedString,
    title: SharedString,
    published: SharedString,
    views: SharedString,
    danmaku: SharedString,
    likes: SharedString,
    description: SharedString,
    pages: Arc<Vec<VideoPage>>,
}

pub(super) struct InfoPanel {
    details: Option<Arc<VideoDetails>>,
    selected_cid: Option<u64>,
    busy: bool,
    scroll: ScrollHandle,
}

impl EventEmitter<InfoEvent> for InfoPanel {}

impl InfoPanel {
    pub(super) fn new() -> Self {
        Self {
            details: None,
            selected_cid: None,
            busy: false,
            scroll: ScrollHandle::default(),
        }
    }

    pub(super) fn is_loaded(&self) -> bool {
        self.details.is_some()
    }

    pub(super) fn clear(&mut self, cx: &mut Context<Self>) {
        *self = Self::new();
        cx.notify();
    }

    pub(super) fn set(&mut self, info: VideoInfo, cid: u64, cx: &mut Context<Self>) {
        let VideoInfo {
            title,
            pubdate,
            owner,
            stat,
            desc,
            pages,
            ..
        } = info;
        self.details = Some(Arc::new(VideoDetails {
            owner: owner.name.into(),
            avatar: owner.face.into(),
            uid: format!("UID {}", owner.mid).into(),
            title: title.into(),
            published: format_datetime(pubdate).into(),
            views: format_count(stat.view).into(),
            danmaku: format_count(stat.danmaku).into(),
            likes: format_count(stat.like).into(),
            description: desc.into(),
            pages: Arc::new(pages),
        }));
        self.selected_cid = Some(cid);
        self.busy = false;
        self.scroll.set_offset(point(px(0.), px(0.)));
        cx.notify();
    }

    pub(super) fn select(&mut self, cid: u64, cx: &mut Context<Self>) {
        self.selected_cid = Some(cid);
        self.busy = false;
        cx.notify();
    }

    pub(super) fn set_busy(&mut self, busy: bool, cx: &mut Context<Self>) {
        if self.busy != busy {
            self.busy = busy;
            cx.notify();
        }
    }

    fn render_pages(&self, details: &VideoDetails, cx: &mut Context<Self>) -> AnyElement {
        let selected = details
            .pages
            .iter()
            .position(|page| Some(page.cid) == self.selected_cid);
        let count = match selected {
            Some(index) => format!("{}/{}", index + 1, details.pages.len()),
            None => format!("共 {} P", details.pages.len()),
        };
        let muted = cx.theme().muted_foreground;
        let radius = cx.theme().radius_lg;
        let weak = cx.entity().downgrade();

        let rows = details.pages.iter().map(|page| {
            let cid = page.cid;
            let current = Some(cid) == self.selected_cid;
            let label = short_label(page);
            let duration = format_duration(page.duration);
            let panel = weak.clone();

            ListItem::new(("page", cid))
                .text_sm()
                .px_2()
                .py_2()
                .selected(current)
                .disabled(self.busy)
                .accessibility_label(label.clone())
                .on_click(move |_, _, cx| {
                    let _ = panel.update(cx, |_, cx| cx.emit(InfoEvent::SelectPage(cid)));
                })
                .child(
                    h_flex()
                        .w_full()
                        .items_center()
                        .gap_2()
                        .child(div().flex_1().min_w_0().truncate().child(label))
                        .child(div().text_xs().text_color(muted).child(duration)),
                )
        });

        v_flex()
            .rounded(radius)
            .overflow_hidden()
            .bg(cx.theme().muted)
            .p_2()
            .gap_1()
            .child(
                h_flex()
                    .items_center()
                    .justify_between()
                    .px_2()
                    .pt_1()
                    .pb_2()
                    .child(
                        div()
                            .text_sm()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child("选集"),
                    )
                    .child(div().text_xs().text_color(muted).child(count)),
            )
            .child(v_flex().gap_1().children(rows))
            .into_any_element()
    }
}

impl Render for InfoPanel {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let background = cx.theme().background;
        let border = cx.theme().border;
        let primary = cx.theme().primary;
        let foreground = cx.theme().foreground;
        let muted = cx.theme().muted_foreground;
        let Some(details) = self.details.clone() else {
            return div().into_any_element();
        };

        let pages = (details.pages.len() > 1).then(|| self.render_pages(&details, cx));
        let description = (!details.description.is_empty()).then(|| {
            div()
                .text_sm()
                .text_color(muted)
                .line_clamp(4)
                .child(details.description.clone())
        });

        v_flex()
            .w(INFO_WIDTH)
            .h_full()
            .flex_shrink_0()
            .bg(background)
            .text_color(foreground)
            .border_l_1()
            .border_color(border)
            .child(
                v_flex()
                    .px_4()
                    .pt_4()
                    .pb_3()
                    .gap_2()
                    .child(
                        div()
                            .text_sm()
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(primary)
                            .child("简介"),
                    )
                    .child(div().h(px(2.)).w(px(24.)).rounded(px(1.)).bg(primary)),
            )
            .child(
                div()
                    .relative()
                    .flex_1()
                    .min_h_0()
                    .child(
                        div()
                            .id("player-info-scroll")
                            .size_full()
                            .overflow_y_scroll()
                            .track_scroll(&self.scroll)
                            .child(
                                v_flex()
                                    .gap_3()
                                    .px_4()
                                    .pb_6()
                                    .child(
                                        h_flex()
                                            .items_center()
                                            .gap_3()
                                            .child(
                                                Avatar::new()
                                                    .src(details.avatar.clone())
                                                    .name(details.owner.clone()),
                                            )
                                            .child(
                                                v_flex()
                                                    .flex_1()
                                                    .min_w_0()
                                                    .gap_1()
                                                    .child(
                                                        div()
                                                            .text_sm()
                                                            .font_weight(FontWeight::SEMIBOLD)
                                                            .truncate()
                                                            .child(details.owner.clone()),
                                                    )
                                                    .child(
                                                        div()
                                                            .text_xs()
                                                            .text_color(muted)
                                                            .child(details.uid.clone()),
                                                    ),
                                            ),
                                    )
                                    .child(
                                        div()
                                            .text_base()
                                            .font_weight(FontWeight::SEMIBOLD)
                                            .line_clamp(2)
                                            .child(details.title.clone()),
                                    )
                                    .child(
                                        h_flex()
                                            .items_center()
                                            .gap_1()
                                            .text_xs()
                                            .text_color(muted)
                                            .child(Icon::new(IconName::Calendar).xsmall())
                                            .child(details.published.clone()),
                                    )
                                    .child(
                                        h_flex()
                                            .items_center()
                                            .py_1()
                                            .child(stat(
                                                IconName::Play,
                                                details.views.clone(),
                                                muted,
                                            ))
                                            .child(stat(
                                                IconName::MessageCircle,
                                                details.danmaku.clone(),
                                                muted,
                                            ))
                                            .child(stat(
                                                IconName::ThumbsUp,
                                                details.likes.clone(),
                                                muted,
                                            )),
                                    )
                                    .when_some(pages, |this, pages| this.child(pages))
                                    .when_some(description, |this, description| {
                                        this.child(description)
                                    }),
                            ),
                    )
                    .child(Scrollbar::vertical(&self.scroll)),
            )
            .into_any_element()
    }
}

fn stat(icon: IconName, value: SharedString, color: Hsla) -> impl IntoElement {
    v_flex()
        .flex_1()
        .items_center()
        .gap_1()
        .child(Icon::new(icon).text_color(color))
        .child(div().text_xs().text_color(color).child(value))
}

fn short_label(page: &VideoPage) -> String {
    if page.part.is_empty() {
        format!("P{}", page.page)
    } else {
        format!("P{} · {}", page.page, page.part)
    }
}

#[cfg(test)]
mod tests {
    use super::short_label;
    use rigili_bilibili::VideoPage;

    fn page(number: u32, part: &str) -> VideoPage {
        VideoPage {
            cid: 42,
            page: number,
            part: part.into(),
            duration: 0,
        }
    }

    #[test]
    fn labels_pages_with_part() {
        assert_eq!(short_label(&page(2, "第二局")), "P2 · 第二局");
    }

    #[test]
    fn labels_pages_without_part() {
        assert_eq!(short_label(&page(1, "")), "P1");
    }
}
