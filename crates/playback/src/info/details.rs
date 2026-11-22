use std::f32::consts::{FRAC_PI_2, PI};
use std::time::{Duration, Instant};

use gpui_kit::assets::IconName;
use gpui_kit::base::SelectableText;
use gpui_kit::component::button::*;
use gpui_kit::component::list::ListItem;
use gpui_kit::component::scroll::Scrollbar;
use gpui_kit::component::spinner::Spinner;
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::component::*;
use gpui_kit::prelude::*;
use gpui_kit::*;
use rigili_bilibili::{RecommendVideo, VideoPage, avatar, thumbnail};
use rigili_core::client;
use rigili_core::video::{VideoCard, video_meta, video_title};
use rigili_core::{format_count, format_duration};

use super::coin::CoinDialog;
use super::favorite::FavoriteDialog;
use super::{
    InfoEvent, InfoPanel, RELATED_THUMB_HEIGHT, RELATED_THUMB_WIDTH, VideoDetails, dialog,
};

const LIKE_TAP: Duration = Duration::from_millis(300);
const TRIPLE_HOLD: Duration = Duration::from_millis(2_000);
const TRIPLE_POP: Duration = Duration::from_millis(600);
const RING_STROKE: Pixels = px(2.);
const RING_STEP: f32 = 2.5;

pub(super) struct LikePress {
    at: Instant,
    fired: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum LikeClick {
    Triple,
    Like,
    Ignore,
}

fn like_click(press: Option<&LikePress>, now: Instant) -> LikeClick {
    let Some(press) = press else {
        return LikeClick::Like;
    };
    if press.fired {
        return LikeClick::Ignore;
    }
    let held = now.duration_since(press.at);
    if held >= TRIPLE_HOLD {
        LikeClick::Triple
    } else if held < LIKE_TAP {
        LikeClick::Like
    } else {
        // A hold abandoned before the ring fills is not a like.
        LikeClick::Ignore
    }
}

fn triple_pop_scale(delta: f32) -> f32 {
    if delta < 0.35 {
        1. + 0.25 * (delta / 0.35)
    } else if delta < 0.7 {
        1.25 - 0.29 * ((delta - 0.35) / 0.35)
    } else {
        0.96 + 0.04 * ((delta - 0.7) / 0.3)
    }
}

impl InfoPanel {
    fn begin_like_press(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.like_press = None;
        if self.like.busy
            || self.triple_busy
            || !client::get(cx).is_ok_and(|client| client.has_session())
        {
            return;
        }
        let generation = self.like_press_generation.wrapping_add(1);
        self.like_press_generation = generation;
        self.like_press = Some(LikePress {
            at: Instant::now(),
            fired: false,
        });
        cx.notify();
        self.like_hold = Some(cx.spawn_in(window, async move |this, cx| {
            cx.background_executor().timer(TRIPLE_HOLD).await;
            let _ = this.update_in(cx, |this, window, cx| {
                if this.like_press_generation != generation {
                    return;
                }
                let Some(press) = this.like_press.as_mut() else {
                    return;
                };
                if press.fired {
                    return;
                }
                press.fired = true;
                cx.notify();
                this.triple(window, cx);
            });
        }));
    }

    fn finish_like_press(
        &mut self,
        aid: u64,
        like: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.like_hold = None;
        let Some(press) = self.like_press.take() else {
            self.toggle_like(aid, like, cx);
            return;
        };
        match like_click(Some(&press), Instant::now()) {
            LikeClick::Triple => self.triple(window, cx),
            LikeClick::Like => self.toggle_like(aid, like, cx),
            LikeClick::Ignore => {}
        }
        cx.notify();
    }

    pub(super) fn start_triple_pop(&mut self, cx: &mut Context<Self>) {
        let generation = self.triple_pop_generation.wrapping_add(1);
        self.triple_pop_generation = generation;
        self.triple_pop = Some(generation);
        cx.notify();
        cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(TRIPLE_POP + Duration::from_millis(100))
                .await;
            let _ = this.update(cx, |this, cx| {
                if this.triple_pop == Some(generation) {
                    this.triple_pop = None;
                    cx.notify();
                }
            });
        })
        .detach();
    }

    fn cancel_like_press(&mut self, cx: &mut Context<Self>) {
        if self.like_press.take().is_none() {
            return;
        }
        self.like_hold = None;
        cx.notify();
    }

    fn toggle_like(&mut self, aid: u64, like: bool, cx: &mut Context<Self>) {
        if self.like.busy || self.triple_busy {
            return;
        }
        cx.emit(InfoEvent::ToggleLike { aid, like });
    }

    fn toggle_description(&mut self, cx: &mut Context<Self>) {
        self.description_expanded = !self.description_expanded;
        cx.notify();
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
        let radius = cx.theme().radius;
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
                .py_1()
                .rounded(radius)
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
            .gap_2()
            .child(
                h_flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .text_sm()
                            .font_weight(FontWeight::MEDIUM)
                            .child("选集"),
                    )
                    .child(div().text_xs().text_color(muted).child(count)),
            )
            .child(v_flex().gap_1().children(rows))
            .into_any_element()
    }

    pub(super) fn render_info_tab(
        &self,
        details: &VideoDetails,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = cx.theme();
        let muted = theme.muted_foreground;
        let border = theme.border;
        let link = theme.link;
        let link_hover = theme.link_hover;
        let danger = theme.danger;

        let weak = cx.entity().downgrade();
        let aid = details.aid;
        let owner_mid = details.owner_mid;
        let followed = self.follow.active;
        let liked = self.like.active;
        let coins_given = self.coin.given;
        let favorited = self.favorite.active;
        let triple = self.triple_plan();
        let triple_busy = self.triple_busy;
        let triple_hint: &'static str = if triple.is_empty() {
            "已点赞、投币并收藏"
        } else {
            "长按点赞 2 秒，一键三连"
        };
        let share_url = format!("https://www.bilibili.com/video/{}", details.bvid);

        let owner_meta: SharedString = match (self.followers, self.owner_likes) {
            (Some(followers), Some(likes)) => {
                let followers = format_count(followers);
                let likes = format_count(likes);
                format!("{followers}粉丝 · {likes}点赞").into()
            }
            (Some(followers), None) => format!("{}粉丝", format_count(followers)).into(),
            _ => format!("UID {owner_mid}").into(),
        };

        let follow_button = if followed {
            Button::new("follow").ghost()
        } else {
            Button::new("follow").primary()
        }
        .small()
        .icon(if followed {
            IconName::Check
        } else {
            IconName::Plus
        })
        .label(if followed { "已关注" } else { "关注" })
        .disabled(self.follow.busy)
        .on_click({
            let weak = weak.clone();
            move |_, _, cx| {
                let _ = weak.update(cx, |_, cx| {
                    cx.emit(InfoEvent::ToggleFollow {
                        mid: owner_mid,
                        follow: !followed,
                    });
                });
            }
        });

        let has_description = !details.description.is_empty();
        let expanded = (has_description && self.description_expanded).then(|| {
            v_flex()
                .gap_2()
                .child(
                    h_flex()
                        .items_center()
                        .gap_2()
                        .text_xs()
                        .text_color(muted)
                        .child(
                            div()
                                .cursor(CursorStyle::IBeam)
                                .child(SelectableText::with_handle(
                                    ("info-bvid", aid),
                                    self.selections.bvid.clone(),
                                    details.bvid.clone(),
                                )),
                        )
                        .when(details.no_reprint, |this| {
                            this.child(Icon::new(IconName::Ban).size(px(12.)).text_color(danger))
                                .child(div().cursor(CursorStyle::IBeam).child(
                                    SelectableText::with_handle(
                                        ("info-no-reprint", aid),
                                        self.selections.no_reprint.clone(),
                                        "未经作者授权，禁止转载",
                                    ),
                                ))
                        }),
                )
                .child(
                    div()
                        .text_sm()
                        .text_color(muted)
                        .cursor(CursorStyle::IBeam)
                        .child(SelectableText::with_handle(
                            ("info-description", aid),
                            self.selections.description.clone(),
                            details.description.clone(),
                        )),
                )
        });

        let pages = (details.pages.len() > 1).then(|| self.render_pages(details, cx));

        let related_selection = self.related_selection;
        let related_active = self.related_focus.focused(window);
        let related = (!self.related.is_empty()).then(|| {
            let layout = self.related_layout.clone();
            let scroll = self.scroll.clone();
            v_flex()
                .image_cache(rigili_core::image_cache::related(cx))
                .gap_1()
                .on_children_prepainted(move |bounds, window, _| {
                    let mut layout = layout.borrow_mut();
                    layout.bounds = bounds;
                    layout.offset = scroll.offset().y;
                    if let Some(index) = layout.pending.take()
                        && let Some(row) = layout.bounds.get(index).copied()
                        && reveal_related_row(&scroll, row)
                    {
                        window.request_animation_frame();
                    }
                })
                .track_focus(self.related_focus.handle())
                .when(related_active, |this| this.key_context(crate::shortcuts::RELATED))
                .children(self.related.iter().enumerate().map(|(index, video)| {
                    let bvid = video.bvid.clone();
                    let cid = video.cid;
                    let title = video.title.clone();
                    let owner_mid = video.owner.mid;
                    let owner_name = video.owner.name.clone();
                    let play_weak = weak.clone();
                    let space_weak = weak.clone();
                    related_row(
                        index,
                        video,
                        related_selection == Some(index),
                        related_active,
                        cx,
                        move |event, _, cx| {
                            let _ = play_weak.update(cx, |_, cx| {
                                cx.emit(InfoEvent::PlayRelated {
                                    bvid: bvid.clone(),
                                    cid,
                                    title: title.clone(),
                                    new_tab: rigili_core::opens_new_tab(event),
                                });
                            });
                        },
                        move |_, _, cx| {
                            let _ = space_weak.update(cx, |_, cx| {
                                cx.emit(InfoEvent::OpenSpace {
                                    mid: owner_mid,
                                    name: owner_name.clone(),
                                });
                            });
                        },
                    )
                }))
        });

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
                            .gap_4()
                            .p_4()
                            .pb_6()
                            .child(
                                rigili_core::user::UserRow::new(
                                    avatar(&details.avatar, 96),
                                    details.owner.clone(),
                                )
                                .key(aid)
                                .open(Some(std::rc::Rc::new({
                                    let weak = weak.clone();
                                    let name = details.owner.to_string();
                                    move |_: &mut Window, cx: &mut App| {
                                        let _ = weak.update(cx, |_, cx| {
                                            cx.emit(InfoEvent::OpenSpace {
                                                mid: owner_mid,
                                                name: name.clone(),
                                            });
                                        });
                                    }
                                })))
                                .meta(
                                    div()
                                        .text_xs()
                                        .text_color(muted)
                                        .truncate()
                                        .child(owner_meta),
                                )
                                .trailing(follow_button)
                                .render(cx),
                            )
                            .child(
                                h_flex()
                                    .items_start()
                                    .gap_2()
                                    .child(
                                        div()
                                            .flex_1()
                                            .min_w_0()
                                            .text_base()
                                            .font_weight(FontWeight::MEDIUM)
                                            .line_clamp(2)
                                            .cursor(CursorStyle::IBeam)
                                            .child(SelectableText::with_handle(
                                                ("info-title", aid),
                                                self.selections.title.clone(),
                                                details.title.clone(),
                                            )),
                                    )
                                    .when(has_description, |this| {
                                        this.child(
                                            h_flex()
                                                .id("toggle-description")
                                                .flex_shrink_0()
                                                .items_center()
                                                .gap_1()
                                                .cursor_pointer()
                                                .text_xs()
                                                .text_color(link)
                                                .hover(move |this| this.text_color(link_hover))
                                                .child(if self.description_expanded {
                                                    "收起"
                                                } else {
                                                    "展开"
                                                })
                                                .child(
                                                    Icon::new(if self.description_expanded {
                                                        IconName::ChevronUp
                                                    } else {
                                                        IconName::ChevronDown
                                                    })
                                                    .size(px(12.)),
                                                )
                                                .on_click({
                                                    let weak = weak.clone();
                                                    move |_, _, cx| {
                                                        let _ = weak.update(cx, |this, cx| {
                                                            this.toggle_description(cx);
                                                        });
                                                    }
                                                }),
                                        )
                                    }),
                            )
                            .child(
                                h_flex()
                                    .items_center()
                                    .gap_4()
                                    .text_xs()
                                    .text_color(muted)
                                    .child(plain_stat(IconName::Play, details.views.clone()))
                                    .child(plain_stat(
                                        IconName::MessageCircle,
                                        details.danmaku.clone(),
                                    ))
                                    .child(plain_stat(IconName::Calendar, details.pubdate.clone())),
                            )
                            .when_some(expanded, |this, expanded| this.child(expanded))
                            .when_some(self.refresh_error.clone(), |this, error| {
                                this.child(div().text_xs().text_color(danger).child(error))
                            })
                            .child(
                                h_flex()
                                    .w_full()
                                    .items_stretch()
                                    .child({
                                        let button = action_button(
                                            ("like", aid),
                                            IconName::ThumbsUp,
                                            format_count(self.like.count).into(),
                                            ActionState {
                                                active: liked,
                                                busy: triple_busy,
                                                dimmed: self.like.busy,
                                                pop: self.triple_pop,
                                            },
                                            cx,
                                        )
                                        .tooltip(move |window, cx| {
                                            Tooltip::new(triple_hint).build(window, cx)
                                        })
                                        .on_mouse_down(
                                            MouseButton::Left,
                                            cx.listener(|this, _, window, cx| {
                                                this.begin_like_press(window, cx)
                                            }),
                                        )
                                        .on_mouse_up_out(
                                            MouseButton::Left,
                                            cx.listener(|this, _, _, cx| {
                                                this.cancel_like_press(cx)
                                            }),
                                        )
                                        .on_click(cx.listener(
                                            move |this, event: &ClickEvent, window, cx| match event
                                            {
                                                ClickEvent::Mouse(_) => {
                                                    this.finish_like_press(aid, !liked, window, cx)
                                                }
                                                _ => this.toggle_like(aid, !liked, cx),
                                            },
                                        ));
                                        if self
                                            .like_press
                                            .as_ref()
                                            .is_some_and(|press| !press.fired)
                                        {
                                            button.child(hold_ring(
                                                self.like_press_generation,
                                                cx.theme().primary,
                                                cx.theme().foreground.opacity(0.15),
                                                cx.theme().radius,
                                            ))
                                        } else {
                                            button
                                        }
                                    })
                                    .child(
                                        action_button(
                                            ("coin", aid),
                                            IconName::Coins,
                                            format_count(self.coin.count).into(),
                                            ActionState {
                                                active: coins_given > 0,
                                                dimmed: coins_given >= 2,
                                                pop: self.triple_pop,
                                                ..Default::default()
                                            },
                                            cx,
                                        )
                                        .on_click({
                                            let weak = weak.clone();
                                            move |_, window, cx| {
                                                if coins_given >= 2 {
                                                    return;
                                                }
                                                let _ = weak.update(cx, |this, cx| {
                                                    let Some(details) = this.details.clone() else {
                                                        return;
                                                    };
                                                    let panel = cx.entity();
                                                    let given = this.coin.given;
                                                    let liked = this.like.active;
                                                    let content = cx.new(|_| {
                                                        CoinDialog::new(
                                                            window,
                                                            details.aid,
                                                            details.bvid.clone(),
                                                            panel,
                                                            given,
                                                            liked,
                                                        )
                                                    });
                                                    dialog::open(window, cx, "投币", content);
                                                });
                                            }
                                        }),
                                    )
                                    .child(
                                        action_button(
                                            ("favorite", aid),
                                            IconName::Star,
                                            format_count(self.favorite.count).into(),
                                            ActionState {
                                                active: favorited,
                                                pop: self.triple_pop,
                                                ..Default::default()
                                            },
                                            cx,
                                        )
                                        .on_click({
                                            let weak = weak.clone();
                                            move |_, window, cx| {
                                                let _ = weak.update(cx, |this, cx| {
                                                    let Some(details) = this.details.clone() else {
                                                        return;
                                                    };
                                                    let panel = cx.entity();
                                                    let content = cx.new(|cx| {
                                                        FavoriteDialog::new(
                                                            window,
                                                            cx,
                                                            details.aid,
                                                            panel,
                                                        )
                                                    });
                                                    dialog::open(window, cx, "选择收藏夹", content);
                                                });
                                            }
                                        }),
                                    )
                                    .child(action_button(
                                        ("cache", aid),
                                        IconName::Download,
                                        "缓存".into(),
                                        ActionState {
                                            dimmed: true,
                                            ..Default::default()
                                        },
                                        cx,
                                    ))
                                    .child(
                                        action_button(
                                            ("share", aid),
                                            IconName::Share2,
                                            format_count(self.share_count).into(),
                                            ActionState::default(),
                                            cx,
                                        )
                                        .on_click({
                                            let url = share_url.clone();
                                            move |_, _, cx| {
                                                cx.write_to_clipboard(ClipboardItem::new_string(
                                                    url.clone(),
                                                ));
                                            }
                                        }),
                                    ),
                            )
                            .child(div().h(px(1.)).w_full().bg(border))
                            .when_some(pages, |this, pages| this.child(pages))
                            .when_some(related, |this, related| this.child(related)),
                    ),
            )
            .child(Scrollbar::vertical(&self.scroll))
            .into_any_element()
    }
}

fn plain_stat(icon: IconName, value: SharedString) -> impl IntoElement {
    h_flex()
        .items_center()
        .gap_1()
        .child(Icon::new(icon).size(px(14.)))
        .child(value)
}

enum RingSegment {
    Line {
        from: (f32, f32),
        dir: (f32, f32),
        len: f32,
    },
    Arc {
        center: (f32, f32),
        radius: f32,
        start: f32,
    },
}

impl RingSegment {
    fn len(&self) -> f32 {
        match self {
            Self::Line { len, .. } => *len,
            Self::Arc { radius, .. } => radius * FRAC_PI_2,
        }
    }

    fn sample(&self, u: f32) -> ((f32, f32), (f32, f32)) {
        match self {
            Self::Line { from, dir, len } => (
                (from.0 + dir.0 * len * u, from.1 + dir.1 * len * u),
                (dir.1, -dir.0),
            ),
            Self::Arc {
                center,
                radius,
                start,
            } => {
                let angle = start + FRAC_PI_2 * u;
                let normal = (angle.cos(), angle.sin());
                (
                    (center.0 + normal.0 * radius, center.1 + normal.1 * radius),
                    normal,
                )
            }
        }
    }
}

fn ring_outline(width: f32, height: f32, radius: f32) -> Vec<RingSegment> {
    let radius = radius.clamp(0., width.min(height) / 2.);
    let (w, h, r) = (width, height, radius);
    let line = |from: (f32, f32), to: (f32, f32)| {
        let (dx, dy) = (to.0 - from.0, to.1 - from.1);
        let len = (dx * dx + dy * dy).sqrt();
        RingSegment::Line {
            from,
            dir: if len > 0. {
                (dx / len, dy / len)
            } else {
                (0., 0.)
            },
            len,
        }
    };
    vec![
        line((w / 2., 0.), (w - r, 0.)),
        RingSegment::Arc {
            center: (w - r, r),
            radius: r,
            start: -FRAC_PI_2,
        },
        line((w, r), (w, h - r)),
        RingSegment::Arc {
            center: (w - r, h - r),
            radius: r,
            start: 0.,
        },
        line((w - r, h), (r, h)),
        RingSegment::Arc {
            center: (r, h - r),
            radius: r,
            start: FRAC_PI_2,
        },
        line((0., h - r), (0., r)),
        RingSegment::Arc {
            center: (r, r),
            radius: r,
            start: PI,
        },
        line((r, 0.), (w / 2., 0.)),
    ]
}

/// A closed ring polygon tracing a rounded rect from the top middle clockwise.
///
/// `progress` is the fraction of the perimeter the ribbon covers; the outline
/// starts at the top middle so the fill reads as a counter filling up.
fn ring_polygon(
    width: f32,
    height: f32,
    radius: f32,
    thickness: f32,
    progress: f32,
) -> Vec<(f32, f32)> {
    let progress = progress.clamp(0., 1.);
    if progress <= 0. || width <= 0. || height <= 0. {
        return Vec::new();
    }
    let segments = ring_outline(width, height, radius);
    let total: f32 = segments.iter().map(RingSegment::len).sum();
    if total <= 0. {
        return Vec::new();
    }
    let mut remaining = total * progress;
    let mut samples = Vec::new();
    for segment in &segments {
        let len = segment.len();
        if len <= 0. {
            continue;
        }
        let used = remaining.min(len);
        let steps = ((used / RING_STEP).ceil() as usize).max(1);
        for step in 0..=steps {
            let u = step as f32 / steps as f32 * used / len;
            samples.push(segment.sample(u));
        }
        remaining -= used;
        if remaining <= 0. {
            break;
        }
    }
    let half = thickness / 2.;
    let mut polygon = Vec::with_capacity(samples.len() * 2);
    for &(point, normal) in &samples {
        polygon.push((point.0 + normal.0 * half, point.1 + normal.1 * half));
    }
    for &(point, normal) in samples.iter().rev() {
        polygon.push((point.0 - normal.0 * half, point.1 - normal.1 * half));
    }
    polygon
}

fn hold_ring(generation: u64, fill: Hsla, track: Hsla, radius: Pixels) -> AnyElement {
    let ring = move |progress: f32| {
        canvas(
            |bounds, _, _| bounds,
            move |_, bounds: Bounds<Pixels>, window, _| {
                paint_ring(bounds, progress, radius, fill, track, window);
            },
        )
        .absolute()
        .inset_0()
    };
    ring(0.)
        .with_animation(
            ("triple-ring", generation),
            Animation::new(TRIPLE_HOLD).with_max_fps(30.),
            move |_, delta| ring(delta),
        )
        .into_any_element()
}

fn paint_ring(
    bounds: Bounds<Pixels>,
    progress: f32,
    radius: Pixels,
    fill: Hsla,
    track: Hsla,
    window: &mut Window,
) {
    let inset = RING_STROKE / 2. + px(0.5);
    let width = bounds.size.width - inset * 2.;
    let height = bounds.size.height - inset * 2.;
    if width <= px(0.) || height <= px(0.) {
        return;
    }
    let origin = bounds.origin + point(inset, inset);
    let radius = (radius - inset).max(px(0.));
    paint_outline(origin, width, height, radius, track, 1., window);
    paint_outline(origin, width, height, radius, fill, progress, window);
}

fn paint_outline(
    origin: Point<Pixels>,
    width: Pixels,
    height: Pixels,
    radius: Pixels,
    color: Hsla,
    progress: f32,
    window: &mut Window,
) {
    let points = ring_polygon(
        width.as_f32(),
        height.as_f32(),
        radius.as_f32(),
        RING_STROKE.as_f32(),
        progress,
    );
    if points.len() < 3 {
        return;
    }
    let points = points
        .into_iter()
        .map(|(x, y)| origin + point(px(x), px(y)))
        .collect::<Vec<_>>();
    let mut builder = PathBuilder::default();
    builder.add_polygon(&points, true);
    if let Ok(path) = builder.build() {
        window.paint_path(path, color);
    }
}

#[derive(Default)]
struct ActionState {
    active: bool,
    busy: bool,
    dimmed: bool,
    pop: Option<u64>,
}

fn action_button(
    id: (&'static str, u64),
    icon: IconName,
    label: SharedString,
    state: ActionState,
    cx: &App,
) -> Stateful<Div> {
    let theme = cx.theme();
    let color = if state.active {
        theme.primary
    } else {
        theme.muted_foreground
    };
    let icon = Icon::new(icon).size(px(22.)).text_color(color);
    let icon: AnyElement = match state.pop {
        Some(generation) => icon
            .with_animation(
                ("triple-pop", generation),
                Animation::new(TRIPLE_POP),
                |icon, delta| {
                    let scale = triple_pop_scale(delta);
                    icon.transform(Transformation::scale(size(scale, scale)))
                },
            )
            .into_any_element(),
        None => icon.into_any_element(),
    };
    v_flex()
        .id(id)
        .relative()
        .flex_1()
        .items_center()
        .gap_1()
        .py_2()
        .rounded(theme.radius)
        .cursor_pointer()
        .opacity(if state.dimmed { 0.45 } else { 1. })
        .when(!state.dimmed && !state.busy, |this| {
            this.hover(move |this| this.bg(theme.accent))
        })
        .child(
            div()
                .h(px(22.))
                .flex()
                .items_center()
                .justify_center()
                .when(state.busy, |this| {
                    this.child(Spinner::new().small().color(color))
                })
                .when(!state.busy, |this| this.child(icon)),
        )
        .child(div().text_xs().text_color(color).child(label))
}

fn related_row(
    index: usize,
    video: &RecommendVideo,
    selected: bool,
    active: bool,
    cx: &App,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    on_author_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    let theme = cx.theme();
    let radius = theme.radius;
    let muted = theme.muted_foreground;
    let accent = theme.accent;
    let primary = theme.primary;
    let card = VideoCard::from(video.clone());
    let cover = thumbnail(card.cover(), 240);
    let title = card.title().to_owned();
    let author = card.author().to_owned();
    let author_mid = card.author_mid();
    let duration = card.duration().to_owned();
    let views = card.views().to_owned();
    let danmaku = card.danmaku().to_owned();

    let author_meta = video_meta(("related-author", index as u64), author, None).text_color(muted);
    let author_meta = if author_mid.is_some() {
        author_meta
            .cursor_pointer()
            .hover(move |this| this.text_color(primary))
            .on_click(move |event, window, cx| {
                cx.stop_propagation();
                on_author_click(event, window, cx);
            })
    } else {
        author_meta
    };

    h_flex()
        .id(("related", index))
        .relative()
        .w_full()
        .items_start()
        .gap_2()
        .p_1()
        .rounded(radius)
        .cursor_pointer()
        .hover(move |this| this.bg(accent))
        .on_click(on_click)
        .child(
            div()
                .relative()
                .w(RELATED_THUMB_WIDTH)
                .h(RELATED_THUMB_HEIGHT)
                .flex_shrink_0()
                .rounded(radius)
                .overflow_hidden()
                .bg(muted.opacity(0.12))
                .child(img(cover).size_full().object_fit(ObjectFit::Cover))
                .child(
                    div()
                        .absolute()
                        .right_1()
                        .bottom_1()
                        .px_1()
                        .rounded(px(2.))
                        .bg(hsla(0., 0., 0., 0.65))
                        .text_size(px(10.))
                        .text_color(Hsla::white())
                        .child(duration),
                ),
        )
        .child(
            v_flex()
                .flex_1()
                .min_w_0()
                .gap_1()
                .child(
                    video_title(("related-title", index as u64), title, 2)
                        .text_color(theme.foreground)
                        .hover(move |this| this.text_color(primary)),
                )
                .child(
                    h_flex()
                        .items_center()
                        .gap_1()
                        .text_xs()
                        .text_color(muted)
                        .child(
                            div()
                                .px(px(3.))
                                .rounded(px(2.))
                                .border_1()
                                .border_color(muted.opacity(0.4))
                                .text_size(px(9.))
                                .child("UP"),
                        )
                        .child(author_meta),
                )
                .child(
                    h_flex()
                        .items_center()
                        .gap_2()
                        .text_xs()
                        .text_color(muted)
                        .child(plain_stat(IconName::Play, views.into()))
                        .child(plain_stat(IconName::MessageCircle, danmaku.into())),
                ),
        )
        .when(selected, |this| {
            let color = if active {
                primary
            } else {
                primary.opacity(0.4)
            };
            this.child(
                div()
                    .absolute()
                    .inset_0()
                    .rounded(radius)
                    .border_2()
                    .border_color(color),
            )
        })
}

fn reveal_related_row(scroll: &ScrollHandle, row: Bounds<Pixels>) -> bool {
    let offset = scroll.offset();
    let y = super::scroll_y_to_show(scroll.bounds(), row, offset.y, scroll.max_offset().y);
    if y == offset.y {
        return false;
    }
    scroll.set_offset(point(offset.x, y));
    true
}

pub(crate) fn short_label(page: &VideoPage) -> String {
    if page.part.is_empty() {
        format!("P{}", page.page)
    } else {
        format!("P{} · {}", page.page, page.part)
    }
}

#[cfg(test)]
mod tests {
    use super::{
        LIKE_TAP, LikeClick, LikePress, SelectableText, TRIPLE_HOLD, like_click, ring_polygon,
        short_label, triple_pop_scale,
    };
    use crate::info::InfoSelections;
    use gpui_kit::base::{TextSelection, TextSelectionHandle};
    use gpui_kit::component::*;
    use gpui_kit::prelude::*;
    use gpui_kit::test::TestWindowExt;
    use gpui_kit::{
        Bounds, Context, CursorStyle, IntoElement, MouseButton, Render, TestAppContext,
        TestSupportExt as _, Window, WindowBounds, WindowOptions, div, point, px, size,
    };
    use rigili_bilibili::VideoPage;
    use std::time::{Duration, Instant};

    const BVID: &str = "BV1TgWp6MEHJ";

    #[test]
    fn only_a_short_tap_or_a_full_hold_acts() {
        let now = Instant::now();
        let press = |fired| LikePress { at: now, fired };

        assert_eq!(like_click(None, now), LikeClick::Like);
        assert_eq!(like_click(Some(&press(false)), now), LikeClick::Like);
        assert_eq!(
            like_click(Some(&press(false)), now + LIKE_TAP),
            LikeClick::Ignore
        );
        assert_eq!(
            like_click(
                Some(&press(false)),
                now + TRIPLE_HOLD - Duration::from_millis(1)
            ),
            LikeClick::Ignore
        );
        assert_eq!(
            like_click(Some(&press(false)), now + TRIPLE_HOLD),
            LikeClick::Triple
        );
        assert_eq!(like_click(Some(&press(true)), now), LikeClick::Ignore);
    }

    #[test]
    fn ring_traces_the_button_outline() {
        assert!(ring_polygon(100., 100., 0., 4., 0.).is_empty());

        let half = ring_polygon(100., 100., 8., 4., 0.5);
        assert!(!half.is_empty());
        for &(x, y) in &half {
            assert!((-2. ..=102.).contains(&x));
            assert!((-2. ..=102.).contains(&y));
        }

        let full = ring_polygon(100., 100., 8., 4., 1.);
        let (outer, inner) = (full[0], full[full.len() - 1]);
        assert!((outer.0 - 50.).abs() < 0.1 && (outer.1 + 2.).abs() < 0.1);
        assert!((inner.0 - 50.).abs() < 0.1 && (inner.1 - 2.).abs() < 0.1);
    }

    #[test]
    fn the_triple_pop_overshoots_and_settles() {
        assert_eq!(triple_pop_scale(0.), 1.);
        assert!((triple_pop_scale(0.35) - 1.25).abs() < 1e-5);
        assert!((triple_pop_scale(0.7) - 0.96).abs() < 1e-5);
        assert_eq!(triple_pop_scale(1.), 1.);
    }

    struct Fixture {
        selection: TextSelectionHandle,
    }

    impl Render for Fixture {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            v_flex().p_4().gap_2().child(
                h_flex().text_xs().child(
                    div()
                        .id("bvid-wrap")
                        .cursor(CursorStyle::IBeam)
                        .test_support()
                        .child(SelectableText::with_handle(
                            "bvid",
                            self.selection.clone(),
                            BVID,
                        )),
                ),
            )
        }
    }

    fn page(number: u32, part: &str) -> VideoPage {
        VideoPage {
            cid: 42,
            page: number,
            part: part.into(),
            duration: 0,
        }
    }

    struct GuardFixture {
        selections: InfoSelections,
    }

    impl Render for GuardFixture {
        fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
            v_flex()
                .id("guard-root")
                .p_4()
                .gap_2()
                .on_mouse_up(
                    MouseButton::Left,
                    cx.listener(|this, _, window, cx| {
                        this.selections.clear_spanning_in(window, cx);
                    }),
                )
                .child(
                    div()
                        .id("guard-first")
                        .test_support()
                        .child(SelectableText::with_handle(
                            "guard-title-text",
                            self.selections.title.clone(),
                            "alpha",
                        )),
                )
                .child(
                    div()
                        .id("guard-second")
                        .test_support()
                        .child(SelectableText::with_handle(
                            "guard-description-text",
                            self.selections.description.clone(),
                            "beta",
                        )),
                )
        }
    }

    #[gpui_kit::test]
    fn clears_selections_that_span_blocks(cx: &mut TestAppContext) {
        cx.update(gpui_kit::init);
        let (window, _) = cx.update(|cx| {
            gpui_kit::open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(Bounds::new(
                        Default::default(),
                        size(px(400.), px(200.)),
                    ))),
                    ..Default::default()
                },
                cx,
                |window, cx| {
                    let selections = InfoSelections::new(window, cx);
                    cx.new(|_| GuardFixture { selections })
                },
            )
            .expect("open guard window")
        });

        let (first, second) = cx
            .update_window(window, |_, window, cx| {
                window.render_frame(cx);
                (
                    window.find("guard-first").bounds(),
                    window.find("guard-second").bounds(),
                )
            })
            .expect("guard bounds");
        let start = point(
            first.origin.x + px(1.),
            first.origin.y + first.size.height / 2.,
        );
        let end = point(
            second.origin.x + second.size.width - px(1.),
            second.origin.y + second.size.height / 2.,
        );

        let selected = cx
            .update_window(window, |_, window, cx| {
                window.drag(start, end, cx);
                TextSelection::selected_text(window, cx)
            })
            .expect("guard drag");
        assert_eq!(selected, "");

        let selected = cx
            .update_window(window, |_, window, cx| {
                window.drag(start, point(start.x + px(20.), start.y), cx);
                TextSelection::selected_text(window, cx)
            })
            .expect("inside drag");
        assert!(!selected.is_empty());
    }

    #[test]
    fn labels_pages() {
        assert_eq!(short_label(&page(2, "第二局")), "P2 · 第二局");
        assert_eq!(short_label(&page(1, "")), "P1");
    }

    #[gpui_kit::test]
    fn selects_a_short_text_run(cx: &mut TestAppContext) {
        cx.update(gpui_kit::init);
        let (window, _) = cx.update(|cx| {
            gpui_kit::open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(Bounds::new(
                        Default::default(),
                        size(px(400.), px(200.)),
                    ))),
                    ..Default::default()
                },
                cx,
                |window, cx| {
                    let selection = TextSelectionHandle::new("", cx);
                    selection.refresh_window_on_change(window, cx).detach();
                    cx.new(|_| Fixture { selection })
                },
            )
            .expect("open selection window")
        });

        let bounds = cx
            .update_window(window, |_, window, cx| {
                window.render_frame(cx);
                window.find("bvid-wrap").bounds()
            })
            .expect("bvid bounds");
        let y = bounds.origin.y + bounds.size.height / 2.;
        let start = point(bounds.origin.x + px(1.), y);
        let end = point(bounds.origin.x + bounds.size.width - px(1.), y);

        let selected = cx
            .update_window(window, |_, window, cx| {
                window.drag(start, end, cx);
                TextSelection::selected_text(window, cx)
            })
            .expect("select bvid");
        assert_eq!(selected, BVID);
    }
}
