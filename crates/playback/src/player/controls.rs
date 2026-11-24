use std::time::Duration;

use gpui_kit::assets::IconName;
use gpui_kit::component::button::*;
use gpui_kit::component::hover_card::HoverCard;
use gpui_kit::component::input::InputState;
use gpui_kit::component::list::ListItem;
use gpui_kit::component::popover::Popover;
use gpui_kit::component::slider::*;
use gpui_kit::component::*;
use gpui_kit::prelude::*;
use gpui_kit::*;
use rigili_bilibili::{Chapter, SubtitleTrack, VideoPage};
use rigili_player::{BufferedRange, PlayerStatus};

use super::danmaku::bar::DanmakuBar;
use super::surface::{BarOverlay, BarOverlays, PlayerView};
use crate::info::short_label;
use crate::request::{AudioKind, AudioTrack, VideoQuality};
use crate::settings::DanmakuSettings;
use crate::shortcuts::{ToggleFullscreen, TogglePlayback, ToggleSubtitles};
use gpui_kit::base::{Slider as BaseSlider, SliderIndicator, SliderThumb, SliderTrack};
use rigili_core::format_clock;

pub(super) struct Transport<'a> {
    pub(super) status: &'a PlayerStatus,
    pub(super) seeking: Option<f64>,
    pub(super) progress: &'a Entity<SliderState>,
    pub(super) progress_active: bool,
    pub(super) chapters: &'a [Chapter],
    pub(super) volume_popup: Entity<VolumePopup>,
    pub(super) qualities: &'a [VideoQuality],
    pub(super) selected_quality: u32,
    pub(super) audio_tracks: &'a [AudioTrack],
    pub(super) selected_audio: Option<AudioKind>,
    pub(super) overlays: BarOverlays,
    pub(super) subtitles: &'a [SubtitleTrack],
    pub(super) selected_subtitle: Option<usize>,
    pub(super) pages: &'a [VideoPage],
    pub(super) selected_cid: Option<u64>,
    pub(super) speeds: &'a [f64],
    pub(super) preview: Option<AnyElement>,
    pub(super) live: bool,
    pub(super) danmaku: DanmakuSettings,
    pub(super) danmaku_input: &'a Entity<InputState>,
    pub(super) danmaku_sending: bool,
    pub(super) can_send_danmaku: bool,
    pub(super) player: WeakEntity<PlayerView>,
}

pub(super) struct VolumePopup {
    volume: Entity<SliderState>,
    _sub: Subscription,
}

impl VolumePopup {
    pub(super) fn new(volume: Entity<SliderState>, cx: &mut Context<Self>) -> Self {
        let sub = cx.subscribe(&volume, |_, _, _: &SliderEvent, cx| cx.notify());
        Self { volume, _sub: sub }
    }
}

impl Render for VolumePopup {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let value = match self.volume.read(cx).value() {
            SliderValue::Single(value) => value,
            SliderValue::Range(start, _) => start,
        };
        let muted = cx.theme().muted_foreground;

        v_flex()
            .items_center()
            .gap_2()
            .child(
                div()
                    .text_xs()
                    .text_color(muted)
                    .child(format!("{value:.0}")),
            )
            .child(
                Slider::new(&self.volume)
                    .vertical()
                    .accessibility_label("音量"),
            )
    }
}

const BACKGROUND_FADE: f32 = 0.92;
const CHAPTER_GAP: f32 = 0.003;
const PROGRESS_OVERHANG: Pixels = px(12.);
const PROGRESS_HIT_HEIGHT: Pixels = px(24.);
const PROGRESS_THUMB: Pixels = px(20.);

const THUMB_ICON: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 22 22"><path d="M16.118 3.667h.382a3.667 3.667 0 0 1 3.667 3.667v7.333a3.667 3.667 0 0 1-3.667 3.667h-11a3.667 3.667 0 0 1-3.667-3.667V7.333A3.667 3.667 0 0 1 5.5 3.666h.382L4.95 2.053a1.1 1.1 0 0 1 1.906-1.1l1.567 2.714h5.156L15.146.953a1.101 1.101 0 0 1 1.906 1.1l-.934 1.614z" fill="#333"/><path d="M5.561 5.194h10.878a2.2 2.2 0 0 1 2.2 2.2v7.211a2.2 2.2 0 0 1-2.2 2.2H5.561a2.2 2.2 0 0 1-2.2-2.2V7.394a2.2 2.2 0 0 1 2.2-2.2z" fill="#fff"/><path d="M6.967 8.556a1.1 1.1 0 0 1 1.1 1.1v2.689a1.1 1.1 0 1 1-2.2 0V9.656a1.1 1.1 0 0 1 1.1-1.1zM15.033 8.556a1.1 1.1 0 0 1 1.1 1.1v2.689a1.1 1.1 0 1 1-2.2 0V9.656a1.1 1.1 0 0 1 1.1-1.1z" fill="#333"/></svg>"##;

const SUBTITLE_ICON: &[u8] = include_bytes!("../../assets/icons/subtitle.svg");
const SUBTITLE_OFF_ICON: &[u8] = include_bytes!("../../assets/icons/subtitle-off.svg");

struct ProgressSegment {
    start: f64,
    end: f64,
    left: f32,
    width: f32,
}

fn progress_bar(
    state: &Entity<SliderState>,
    active: bool,
    buffered: &[BufferedRange],
    chapters: &[Chapter],
    duration: f64,
    cx: &App,
) -> impl IntoElement {
    let theme = cx.theme();
    let percentage = state.read(cx).percentage();
    let height = if active { px(4.) } else { px(2.) };
    let track = theme.foreground.opacity(if active { 0.3 } else { 0.2 });
    let buffered_color = theme.foreground.opacity(if active { 0.5 } else { 0.32 });
    let played_from = f64::from(percentage.start) * duration;
    let played_to = f64::from(percentage.end) * duration;
    let segments = progress_segments(chapters, duration);
    let segmented = segments.len() > 1;
    let thumb = active.then(|| {
        SliderThumb::new(state)
            .axis(Axis::Horizontal)
            .absolute()
            .top((PROGRESS_HIT_HEIGHT - PROGRESS_THUMB) / 2.)
            .left(relative(percentage.end))
            .ml(-PROGRESS_THUMB / 2.)
            .size(PROGRESS_THUMB)
            .child(
                Icon::default()
                    .data(THUMB_ICON.as_bytes())
                    .size(PROGRESS_THUMB),
            )
    });

    BaseSlider::new(state)
        .axis(Axis::Horizontal)
        .accessibility_label("播放进度")
        .flex()
        .items_center()
        .w_full()
        .h(PROGRESS_HIT_HEIGHT)
        .child(
            SliderTrack::new(state)
                .axis(Axis::Horizontal)
                .relative()
                .flex()
                .items_center()
                .w_full()
                .h(PROGRESS_HIT_HEIGHT)
                .child(
                    SliderIndicator::new(state)
                        .relative()
                        .w_full()
                        .h(height)
                        .rounded_full()
                        .children(segments.into_iter().map(|segment| {
                            let span = segment.end - segment.start;
                            let local = |time: f64| {
                                if span > 0.0 {
                                    ((time - segment.start) / span).clamp(0.0, 1.0) as f32
                                } else {
                                    0.0
                                }
                            };
                            let buffers = buffered.iter().filter_map(|range| {
                                let start = range.start.max(segment.start);
                                let end = range.end.min(segment.end);
                                (end > start).then(|| (local(start), local(end)))
                            });
                            let played = (span > 0.0
                                && played_to > segment.start
                                && played_from < segment.end)
                                .then(|| {
                                    (
                                        local(played_from.max(segment.start)),
                                        local(played_to.min(segment.end)),
                                    )
                                });
                            let right = (1.0 - segment.left - segment.width).clamp(0.0, 1.0);

                            div()
                                .absolute()
                                .h_full()
                                .left(relative(segment.left))
                                .right(relative(right))
                                .overflow_hidden()
                                .when(!segmented, |this| this.rounded_full())
                                .bg(track)
                                .children(buffers.map(|(start, end)| {
                                    div()
                                        .absolute()
                                        .h_full()
                                        .left(relative(start))
                                        .right(relative(1. - end))
                                        .bg(buffered_color)
                                }))
                                .when_some(played, |this, (start, end)| {
                                    this.child(
                                        div()
                                            .absolute()
                                            .h_full()
                                            .left(relative(start))
                                            .right(relative(1. - end))
                                            .bg(theme.primary),
                                    )
                                })
                        })),
                )
                .when_some(thumb, |this, thumb| this.child(thumb)),
        )
}

fn progress_segments(chapters: &[Chapter], duration: f64) -> Vec<ProgressSegment> {
    if duration <= 0.0 {
        return vec![ProgressSegment {
            start: 0.0,
            end: 0.0,
            left: 0.0,
            width: 1.0,
        }];
    }

    let mut bounds = chapters
        .iter()
        .filter_map(|chapter| {
            let start = chapter.start.clamp(0.0, duration);
            let end = chapter.end.clamp(0.0, duration);
            (end > start).then_some((start, end))
        })
        .collect::<Vec<_>>();
    bounds.sort_by(|a, b| a.0.total_cmp(&b.0));

    let total = bounds.iter().map(|(start, end)| end - start).sum::<f64>();
    if bounds.is_empty() || total <= 0.0 {
        return vec![ProgressSegment {
            start: 0.0,
            end: duration,
            left: 0.0,
            width: 1.0,
        }];
    }

    let scale = (1.0 - CHAPTER_GAP * (bounds.len() as f32 - 1.0)).max(0.0);
    let mut occupied = 0.0;
    bounds
        .into_iter()
        .enumerate()
        .map(|(index, (start, end))| {
            let left = occupied + CHAPTER_GAP * index as f32;
            let width = ((end - start) / total) as f32 * scale;
            occupied += width;
            ProgressSegment {
                start,
                end,
                left,
                width,
            }
        })
        .collect()
}

impl Transport<'_> {
    pub(super) fn render(&mut self, window: &Window, cx: &App) -> impl IntoElement {
        let theme = cx.theme();
        let muted = theme.muted_foreground;
        let primary = theme.primary;
        let background = theme.background;
        let border = theme.border;
        let weak = self.player.clone();
        let display_time = self.seeking.unwrap_or(self.status.time_pos);
        let duration = self.status.duration.max(0.0);
        let paused = self.status.paused;
        let speed = self.status.speed;
        let volume_icon = volume_icon(self.status.volume, self.status.muted);
        let quality = self
            .qualities
            .iter()
            .find(|quality| quality.id == self.selected_quality)
            .filter(|_| self.qualities.len() > 1)
            .map(|quality| quality.short_label.clone());
        let quality_tooltip = match self.status.video_size {
            Some(size) if size.width > 0 && size.height > 0 => {
                format!("{}×{} · 切换清晰度", size.width, size.height)
            }
            _ => "切换清晰度".to_owned(),
        };
        let audio_label = self
            .audio_tracks
            .iter()
            .find(|track| Some(track.kind) == self.selected_audio)
            .filter(|_| self.audio_tracks.len() > 1)
            .map(|track| audio_short_label(track.kind));
        let fullscreen = window.is_fullscreen();
        let subtitle_on = self.selected_subtitle.is_some();
        let current_page = page_label(self.pages, self.selected_cid);
        let preview = self.preview.take();
        let progress = (!self.live).then(|| {
            div()
                .id("player-progress")
                .absolute()
                .left_3()
                .right_3()
                .top(-PROGRESS_OVERHANG)
                .on_prepaint({
                    let weak = weak.clone();
                    move |bounds, _, cx| {
                        let _ = weak.update(cx, |this, _| this.set_progress_bounds(bounds));
                    }
                })
                .on_mouse_move({
                    let weak = weak.clone();
                    move |event: &MouseMoveEvent, _, cx| {
                        let _ = weak.update(cx, |this, cx| this.preview_at(event.position, cx));
                    }
                })
                .on_hover({
                    let weak = weak.clone();
                    move |hovered, _, cx| {
                        let _ = weak.update(cx, |this, cx| this.preview_hovered(*hovered, cx));
                    }
                })
                .child(progress_bar(
                    self.progress,
                    self.progress_active,
                    &self.status.buffered,
                    self.chapters,
                    duration,
                    cx,
                ))
                .children(preview)
        });

        div()
            .relative()
            .w_full()
            .bg(linear_gradient(
                180.,
                linear_color_stop(background.opacity(0.), 0.),
                linear_color_stop(background.opacity(BACKGROUND_FADE), 1.),
            ))
            .children(progress)
            .child(
                h_flex()
                    .w_full()
                    .items_center()
                    .gap_3()
                    .px_3()
                    .pt_8()
                    .pb_2()
                    .child(
                        Button::new("toggle-pause")
                            .ghost()
                            .with_size(px(32.))
                            .icon(if paused {
                                IconName::Play
                            } else {
                                IconName::Pause
                            })
                            .accessibility_label(if paused { "播放" } else { "暂停" })
                            .tooltip_with_action(
                                if paused { "播放" } else { "暂停" },
                                &TogglePlayback,
                                Some(crate::shortcuts::PLAYER),
                            )
                            .on_click(|_, window, cx| {
                                window.dispatch_action(Box::new(TogglePlayback), cx);
                            }),
                    )
                    .when(!self.live, |this| {
                        this.child(div().text_sm().text_color(muted).child(format!(
                            "{} / {}",
                            format_clock(display_time),
                            format_clock(duration)
                        )))
                    })
                    .when(self.live, |this| {
                        this.child(
                            h_flex()
                                .items_center()
                                .gap_2()
                                .text_sm()
                                .text_color(primary)
                                .child(div().size_2().rounded_full().bg(primary))
                                .child("直播中"),
                        )
                    })
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .px_4()
                            .flex()
                            .justify_center()
                            .child(
                                DanmakuBar {
                                    settings: self.danmaku,
                                    input: self.danmaku_input,
                                    sending: self.danmaku_sending,
                                    settings_open: self
                                        .overlays
                                        .is_open(BarOverlay::DanmakuSettings),
                                    can_send: self.can_send_danmaku,
                                    player: weak.clone(),
                                }
                                .render(cx),
                            ),
                    )
                    .child(
                        HoverCard::new("player-volume")
                            .anchor(Anchor::TopCenter)
                            .open_delay(Duration::from_millis(120))
                            .close_delay(Duration::from_millis(200))
                            .trigger(
                                div()
                                    .px_2()
                                    .py_1()
                                    .child(Icon::new(volume_icon).size(px(20.)).text_color(muted)),
                            )
                            .content({
                                let popup = self.volume_popup.clone();
                                move |_, _, _| popup.clone()
                            })
                            .on_open_change({
                                let weak = weak.clone();
                                move |open, _, cx| {
                                    let _ = weak.update(cx, |this, cx| {
                                        this.set_bar_overlay(BarOverlay::Volume, *open, cx);
                                    });
                                }
                            }),
                    )
                    .when_some(quality, |this, current| {
                        this.child(bar_popover(
                            BarOverlay::Quality,
                            Anchor::BottomRight,
                            &weak,
                            self.overlays.is_open(BarOverlay::Quality),
                            Button::new("quality")
                                .ghost()
                                .small()
                                .label(current)
                                .tooltip(quality_tooltip),
                            v_flex()
                                .min_w(px(120.))
                                .gap_1()
                                .children(self.qualities.iter().map(|quality| {
                                    let selected = quality.id == self.selected_quality;
                                    ListItem::new(("quality", quality.id))
                                        .check_icon(IconName::Check)
                                        .confirmed(selected)
                                        .selected(selected)
                                        .on_click({
                                            let weak = weak.clone();
                                            let id = quality.id;
                                            move |_, _, cx| {
                                                let _ = weak.update(cx, |this, cx| {
                                                    this.switch_quality(id, cx);
                                                });
                                            }
                                        })
                                        .child(quality.label.clone())
                                })),
                        ))
                    })
                    .when_some(audio_label, |this, current| {
                        this.child(bar_popover(
                            BarOverlay::Audio,
                            Anchor::BottomRight,
                            &weak,
                            self.overlays.is_open(BarOverlay::Audio),
                            Button::new("audio")
                                .ghost()
                                .small()
                                .label(current)
                                .tooltip("切换音质"),
                            v_flex().min_w(px(140.)).gap_1().children(
                                self.audio_tracks.iter().map(|track| {
                                    let selected = Some(track.kind) == self.selected_audio;
                                    ListItem::new(("audio", track.id))
                                        .check_icon(IconName::Check)
                                        .confirmed(selected)
                                        .selected(selected)
                                        .on_click({
                                            let weak = weak.clone();
                                            let kind = track.kind;
                                            move |_, _, cx| {
                                                let _ = weak.update(cx, |this, cx| {
                                                    this.switch_audio(kind, cx);
                                                });
                                            }
                                        })
                                        .child(track.label.clone())
                                }),
                            ),
                        ))
                    })
                    .when(!self.subtitles.is_empty() && !self.live, |this| {
                        this.child(bar_popover(
                            BarOverlay::Subtitle,
                            Anchor::BottomRight,
                            &weak,
                            self.overlays.is_open(BarOverlay::Subtitle),
                            Button::new("subtitles")
                                .ghost()
                                .with_size(px(32.))
                                .icon(subtitle_icon(
                                    subtitle_on,
                                    px(18.),
                                    if subtitle_on { primary } else { muted },
                                ))
                                .text_color(if subtitle_on { primary } else { muted })
                                .accessibility_label("字幕")
                                .tooltip_with_action(
                                    "字幕",
                                    &ToggleSubtitles,
                                    Some(crate::shortcuts::PLAYER),
                                ),
                            v_flex()
                                .min_w(px(160.))
                                .max_w(px(240.))
                                .gap_2()
                                .child(
                                    v_flex()
                                        .id("subtitle-list")
                                        .max_h(px(240.))
                                        .overflow_y_scroll()
                                        .gap_1()
                                        .child(
                                            ListItem::new("subtitle-off")
                                                .check_icon(IconName::Check)
                                                .confirmed(self.selected_subtitle.is_none())
                                                .selected(self.selected_subtitle.is_none())
                                                .on_click({
                                                    let weak = weak.clone();
                                                    move |_, _, cx| {
                                                        let _ = weak.update(cx, |this, cx| {
                                                            this.select_subtitle(None, cx);
                                                        });
                                                    }
                                                })
                                                .child("关闭"),
                                        )
                                        .children(self.subtitles.iter().enumerate().map(
                                            |(index, subtitle)| {
                                                let selected =
                                                    self.selected_subtitle == Some(index);
                                                ListItem::new(("subtitle", index as u32))
                                                    .check_icon(IconName::Check)
                                                    .confirmed(selected)
                                                    .selected(selected)
                                                    .on_click({
                                                        let weak = weak.clone();
                                                        move |_, _, cx| {
                                                            let _ = weak.update(cx, |this, cx| {
                                                                this.select_subtitle(
                                                                    Some(index),
                                                                    cx,
                                                                );
                                                            });
                                                        }
                                                    })
                                                    .child(subtitle.label().to_owned())
                                            },
                                        )),
                                )
                                .child(v_flex().gap_1().when(subtitle_on, |this| {
                                    this.child(
                                        div()
                                            .pt_2()
                                            .border_t_1()
                                            .border_color(border)
                                            .text_xs()
                                            .text_color(muted)
                                            .child("显示时可直接拖动字幕调整位置"),
                                    )
                                })),
                        ))
                    })
                    .when_some(current_page, |this, current| {
                        this.child(bar_popover(
                            BarOverlay::Pages,
                            Anchor::BottomRight,
                            &weak,
                            self.overlays.is_open(BarOverlay::Pages),
                            Button::new("player-page")
                                .ghost()
                                .small()
                                .label(current)
                                .tooltip("选集"),
                            v_flex()
                                .id("player-page-list")
                                .min_w(px(160.))
                                .max_w(px(320.))
                                .max_h(px(360.))
                                .overflow_y_scroll()
                                .gap_1()
                                .children(self.pages.iter().map(|page| {
                                    let selected = Some(page.cid) == self.selected_cid;
                                    let cid = page.cid;
                                    let label = short_label(page);
                                    ListItem::new(("player-page-item", cid))
                                        .text_sm()
                                        .check_icon(IconName::Check)
                                        .confirmed(selected)
                                        .selected(selected)
                                        .on_click({
                                            let weak = weak.clone();
                                            move |_, _, cx| {
                                                let _ = weak.update(cx, |this, cx| {
                                                    this.request_page(cid, cx);
                                                });
                                            }
                                        })
                                        .child(div().min_w_0().truncate().child(label))
                                })),
                        ))
                    })
                    .when(!self.live, |this| {
                        this.child(bar_popover(
                            BarOverlay::Speed,
                            Anchor::BottomRight,
                            &weak,
                            self.overlays.is_open(BarOverlay::Speed),
                            Button::new("speed")
                                .ghost()
                                .small()
                                .label(speed_label(speed))
                                .tooltip("切换倍速"),
                            v_flex().min_w(px(96.)).gap_1().children(
                                self.speeds.iter().enumerate().rev().map(|(index, value)| {
                                    let selected = (speed - *value).abs() < 1e-3;
                                    ListItem::new(("speed", index as u32))
                                        .check_icon(IconName::Check)
                                        .confirmed(selected)
                                        .selected(selected)
                                        .on_click({
                                            let weak = weak.clone();
                                            let value = *value;
                                            move |_, _, cx| {
                                                let _ = weak.update(cx, |this, cx| {
                                                    this.select_speed(value, cx);
                                                });
                                            }
                                        })
                                        .child(speed_label(*value))
                                }),
                            ),
                        ))
                    })
                    .child(
                        Button::new("fullscreen")
                            .ghost()
                            .with_size(px(32.))
                            .icon(if fullscreen {
                                IconName::Minimize
                            } else {
                                IconName::Maximize
                            })
                            .accessibility_label(if fullscreen { "退出全屏" } else { "全屏" })
                            .tooltip_with_action(
                                if fullscreen { "退出全屏" } else { "全屏" },
                                &ToggleFullscreen,
                                Some(crate::shortcuts::PLAYER),
                            )
                            .on_click(|_, window, cx| {
                                window.dispatch_action(Box::new(ToggleFullscreen), cx);
                            }),
                    ),
            )
    }
}

fn bar_popover(
    overlay: BarOverlay,
    anchor: Anchor,
    weak: &WeakEntity<PlayerView>,
    open: bool,
    trigger: Button,
    content: impl IntoElement + 'static,
) -> Popover {
    let weak = weak.clone();
    Popover::new(overlay.id())
        .anchor(anchor)
        .offset(px(6.))
        .open(open)
        .trigger(trigger)
        .on_open_change(move |open, _, cx| {
            let _ = weak.update(cx, |this, cx| {
                this.set_bar_overlay(overlay, *open, cx);
            });
        })
        .child(content)
}

fn audio_short_label(kind: AudioKind) -> &'static str {
    match kind {
        AudioKind::Regular => "普通",
        AudioKind::Dolby => "杜比全景声",
        AudioKind::Flac => "Hi-Res",
    }
}

fn subtitle_icon(enabled: bool, size: Pixels, color: Hsla) -> Icon {
    Icon::default()
        .data(if enabled {
            SUBTITLE_ICON
        } else {
            SUBTITLE_OFF_ICON
        })
        .with_size(size)
        .text_color(color)
}

fn volume_icon(volume: f64, muted: bool) -> IconName {
    if muted {
        return IconName::VolumeOff;
    }
    match volume {
        value if value <= 0.0 => IconName::VolumeOff,
        value if value <= 33.0 => IconName::Volume,
        value if value <= 66.0 => IconName::Volume1,
        _ => IconName::Volume2,
    }
}

pub(super) fn speed_label(speed: f64) -> String {
    let text = format!("{speed:.2}");
    let text = text.trim_end_matches('0').trim_end_matches('.');
    format!("{text}x")
}

fn page_label(pages: &[VideoPage], selected: Option<u64>) -> Option<String> {
    if pages.len() <= 1 {
        return None;
    }
    pages
        .iter()
        .find(|page| Some(page.cid) == selected)
        .or_else(|| pages.first())
        .map(|page| format!("P{}", page.page))
}

#[cfg(test)]
mod tests {
    use super::{page_label, progress_segments, speed_label, volume_icon};
    use gpui_kit::assets::IconName;
    use rigili_bilibili::{Chapter, VideoPage};

    #[test]
    fn picks_volume_icons_by_level() {
        assert_eq!(volume_icon(0.0, false), IconName::VolumeOff);
        assert_eq!(volume_icon(50.0, true), IconName::VolumeOff);
        assert_eq!(volume_icon(20.0, false), IconName::Volume);
        assert_eq!(volume_icon(50.0, false), IconName::Volume1);
        assert_eq!(volume_icon(100.0, false), IconName::Volume2);
    }

    #[test]
    fn labels_speeds() {
        assert_eq!(speed_label(1.0), "1x");
        assert_eq!(speed_label(1.25), "1.25x");
    }

    #[test]
    fn labels_the_current_page() {
        let pages = [
            VideoPage {
                cid: 11,
                page: 1,
                part: String::new(),
                duration: 0,
            },
            VideoPage {
                cid: 22,
                page: 2,
                part: "下半场".into(),
                duration: 0,
            },
        ];

        assert_eq!(page_label(&pages, Some(22)).as_deref(), Some("P2"));
        assert_eq!(page_label(&pages, Some(99)).as_deref(), Some("P1"));
        assert_eq!(page_label(&pages[..1], None), None);
    }

    #[test]
    fn lays_out_chapters_with_gaps() {
        let chapters = [
            chapter(50.0, 100.0),
            chapter(0.0, 50.0),
            chapter(100.0, 100.0),
        ];
        let segments = progress_segments(&chapters, 100.0);

        assert_eq!(segments.len(), 2);
        assert_eq!((segments[0].start, segments[0].end), (0.0, 50.0));
        assert_eq!((segments[1].left, segments[1].width), (0.5015, 0.4985));
        assert!((segments[1].left + segments[1].width - 1.0).abs() < 1e-6);
    }

    #[test]
    fn falls_back_to_one_segment() {
        let segments = progress_segments(&[], 100.0);
        assert_eq!(segments.len(), 1);
        assert_eq!((segments[0].start, segments[0].end), (0.0, 100.0));
        assert_eq!((segments[0].left, segments[0].width), (0.0, 1.0));

        let segments = progress_segments(&[chapter(10.0, 20.0)], 0.0);
        assert_eq!(segments.len(), 1);
        assert_eq!((segments[0].start, segments[0].end), (0.0, 0.0));

        let segments = progress_segments(&[chapter(20.0, f64::INFINITY)], 100.0);
        assert_eq!((segments[0].start, segments[0].end), (20.0, 100.0));
    }

    fn chapter(start: f64, end: f64) -> Chapter {
        Chapter {
            start,
            end,
            title: String::new(),
        }
    }
}
