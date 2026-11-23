use gpui_kit::assets::IconName;
use gpui_kit::component::button::*;
use gpui_kit::component::spinner::Spinner;
use gpui_kit::component::tab::{Tab as TabButton, TabBar};
use gpui_kit::component::*;
use gpui_kit::prelude::*;
use gpui_kit::*;
use rigili_core::FocusMain;

use super::{INFO_MIN_WINDOW, PlayerWindow};
use crate::info::InfoTab;
use crate::shortcuts::{
    BackComments, CloseCommentThread, CloseTab, ExpandReplies, HistoryBack, HistoryForward,
    NextComment, NextPage, NextRelated, NextTab, OpenCommentImage, PlayRelated,
    PlayRelatedInNewTab, PreviousComment, PreviousPage, PreviousRelated, PreviousTab, Reload,
    SeekBack, SpeedDown, SpeedUp, SwitchToMain, ToggleComments, ToggleDanmaku, ToggleFullscreen,
    ToggleInfo, ToggleMute, TogglePanelNavigation, TogglePlayback, ToggleSubtitles, WriteComment,
};

impl Render for PlayerWindow {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let background = cx.theme().background;
        let foreground = cx.theme().foreground;
        let muted = cx.theme().muted_foreground;
        let primary = cx.theme().primary;
        let danger = cx.theme().danger;
        let fullscreen = window.is_fullscreen();
        let client_decorated = matches!(window.window_decorations(), Decorations::Client { .. });
        let wide_enough = window.viewport_size().width >= INFO_MIN_WINDOW;
        let (title, can_go_back, can_go_forward, is_live, loading, error, info) = {
            let tab = self.tab();
            (
                tab.title.clone(),
                tab.history.can_back(),
                tab.history.can_forward(),
                tab.is_live,
                tab.loading,
                tab.error.clone(),
                tab.info.clone(),
            )
        };
        let show_info = self.show_info && !is_live && (fullscreen || wide_enough);
        let weak = cx.entity().downgrade();

        if window.window_title() != title.as_ref() {
            window.set_window_title(title.as_ref());
        }

        let tabs = TabBar::new("player-tabs")
            .selected_index(self.active)
            .max_width(px(200.))
            .children(self.tabs.iter().enumerate().map(|(index, tab)| {
                let audio = tab.background_playing;
                TabButton::new().label(tab.title.clone()).suffix(
                    h_flex()
                        .items_center()
                        .gap_1()
                        .when(audio, |this| {
                            this.child(
                                Icon::new(IconName::AudioLines)
                                    .size(px(14.))
                                    .text_color(primary),
                            )
                        })
                        .child(
                            Button::new(("player-tab-close", tab.id))
                                .ghost()
                                .small()
                                .icon(IconName::Close)
                                .accessibility_label("关闭标签页")
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    cx.stop_propagation();
                                    this.close_tab(index, window, cx);
                                })),
                        ),
                )
            }))
            .on_click(cx.listener(|this, index: &usize, _, cx| this.select_tab(*index, cx)));

        v_flex()
            .image_cache(rigili_core::image_cache::get(cx))
            .id("player-window")
            .size_full()
            .bg(background)
            .text_color(foreground)
            .key_context(crate::shortcuts::PLAYER)
            .capture_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                if event.keystroke.key == "escape"
                    && (this.player.read(cx).text_input_focused(window, cx)
                        || this.tab().info.read(cx).text_input_focused(window, cx))
                {
                    window.blur(cx);
                    this.player
                        .update(cx, |player, cx| player.focus_player(window, cx));
                    cx.stop_propagation();
                }
            }))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _, window, cx| {
                    this.player
                        .update(cx, |player, cx| player.focus_player(window, cx));
                }),
            )
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, _, window, cx| {
                    this.tab().info.update(cx, |info, cx| {
                        info.clear_spanning_selection(window, cx);
                    });
                }),
            )
            .on_action(cx.listener(|this, _: &TogglePlayback, _, cx| {
                this.player.update(cx, |player, cx| player.toggle_pause(cx));
            }))
            .on_action(cx.listener(|this, _: &SeekBack, _, cx| {
                this.player.update(cx, |player, cx| player.seek_by(-5., cx));
            }))
            .on_action(cx.listener(|this, _: &HistoryBack, window, cx| {
                if this.close_comment_thread(window, cx) {
                    return;
                }
                this.go_back(cx);
            }))
            .on_action(cx.listener(|this, _: &CloseCommentThread, window, cx| {
                if this.exit_related(window, cx) {
                    return;
                }
                this.close_comment_thread(window, cx);
            }))
            .on_action(cx.listener(|this, _: &HistoryForward, _, cx| {
                this.go_forward(cx);
            }))
            .on_action(cx.listener(|this, _: &SpeedDown, _, cx| {
                this.player
                    .update(cx, |player, cx| player.adjust_speed(false, cx));
            }))
            .on_action(cx.listener(|this, _: &SpeedUp, _, cx| {
                this.player
                    .update(cx, |player, cx| player.adjust_speed(true, cx));
            }))
            .on_action(cx.listener(|this, _: &PreviousPage, _, cx| {
                this.step_page(-1, cx);
            }))
            .on_action(cx.listener(|this, _: &NextPage, _, cx| {
                this.step_page(1, cx);
            }))
            .on_action(cx.listener(|this, _: &ToggleMute, _, cx| {
                this.player.update(cx, |player, cx| player.toggle_mute(cx));
            }))
            .on_action(cx.listener(|this, _: &ToggleFullscreen, window, cx| {
                this.player
                    .update(cx, |player, cx| player.toggle_fullscreen(window, cx));
            }))
            .on_action(cx.listener(|this, _: &ToggleInfo, _, cx| {
                this.toggle_panel(InfoTab::Info, cx);
            }))
            .on_action(cx.listener(|this, _: &TogglePanelNavigation, window, cx| {
                this.toggle_panel_navigation(window, cx);
            }))
            .on_action(cx.listener(|this, _: &PreviousRelated, _, cx| {
                this.step_related(-1, cx);
            }))
            .on_action(cx.listener(|this, _: &NextRelated, _, cx| {
                this.step_related(1, cx);
            }))
            .on_action(cx.listener(|this, _: &PlayRelated, _, cx| {
                this.play_related(false, cx);
            }))
            .on_action(cx.listener(|this, _: &PlayRelatedInNewTab, _, cx| {
                this.play_related(true, cx);
            }))
            .on_action(cx.listener(|this, _: &PreviousComment, _, cx| {
                this.step_comment(-1, cx);
            }))
            .on_action(cx.listener(|this, _: &NextComment, _, cx| {
                this.step_comment(1, cx);
            }))
            .on_action(cx.listener(|this, _: &ExpandReplies, window, cx| {
                this.expand_comment(window, cx);
            }))
            .on_action(cx.listener(|this, _: &OpenCommentImage, window, cx| {
                let info = this.tab().info.clone();
                info.update(cx, |info, cx| {
                    info.open_selected_comment_picture(window, cx)
                });
            }))
            .on_action(cx.listener(|this, _: &BackComments, window, cx| {
                this.back_comment(window, cx);
            }))
            .on_action(cx.listener(|this, _: &ToggleComments, _, cx| {
                this.toggle_panel(InfoTab::Comments, cx);
            }))
            .on_action(cx.listener(|this, _: &WriteComment, window, cx| {
                let visible = this.show_info
                    && !this.tab().is_live
                    && (window.is_fullscreen() || window.viewport_size().width >= INFO_MIN_WINDOW)
                    && this.tab().info.read(cx).tab() == InfoTab::Comments;
                if !visible {
                    return;
                }
                let info = this.tab().info.clone();
                info.update(cx, |info, cx| info.focus_composer(window, cx));
            }))
            .on_action(cx.listener(|this, _: &ToggleDanmaku, _, cx| {
                this.player
                    .update(cx, |player, cx| player.toggle_danmaku(cx));
            }))
            .on_action(cx.listener(|this, _: &ToggleSubtitles, _, cx| {
                this.player
                    .update(cx, |player, cx| player.toggle_subtitles(cx));
            }))
            .on_action(cx.listener(|this, _: &Reload, _, cx| {
                this.reload(cx);
            }))
            .on_action(cx.listener(|this, _: &CloseTab, window, cx| {
                this.close_tab(this.active, window, cx);
            }))
            .on_action(cx.listener(|this, _: &NextTab, _, cx| {
                if this.tabs.len() > 1 {
                    this.select_tab((this.active + 1) % this.tabs.len(), cx);
                }
            }))
            .on_action(cx.listener(|this, _: &PreviousTab, _, cx| {
                if this.tabs.len() > 1 {
                    this.select_tab((this.active + this.tabs.len() - 1) % this.tabs.len(), cx);
                }
            }))
            .on_action(cx.listener(|_, _: &SwitchToMain, _, cx| {
                cx.emit(FocusMain);
            }))
            .on_action(
                cx.listener(|_, _: &rigili_core::ShowShortcuts, window, cx| {
                    let rows = crate::shortcuts::rows(cx);
                    rigili_core::open_shortcut_help(window, cx, "播放器快捷键", rows);
                }),
            )
            .when(!fullscreen, |this| {
                this.child(
                    TitleBar::new().child(
                        h_flex()
                            .w_full()
                            .items_center()
                            .gap_2()
                            .pr_6()
                            .child(
                                h_flex()
                                    .items_center()
                                    .gap_1()
                                    .child(
                                        Button::new("player-back")
                                            .ghost()
                                            .icon(IconName::ChevronLeft)
                                            .accessibility_label("上一个视频")
                                            .tooltip("上一个视频")
                                            .disabled(!can_go_back)
                                            .on_click(
                                                cx.listener(|this, _, _, cx| this.go_back(cx)),
                                            ),
                                    )
                                    .child(
                                        Button::new("player-forward")
                                            .ghost()
                                            .icon(IconName::ChevronRight)
                                            .accessibility_label("下一个视频")
                                            .tooltip("下一个视频")
                                            .disabled(!can_go_forward)
                                            .on_click(
                                                cx.listener(|this, _, _, cx| this.go_forward(cx)),
                                            ),
                                    ),
                            )
                            .child(div().flex_1().min_w_0().child(tabs))
                            .when(!client_decorated, |this| {
                                this.child(
                                    Button::new("window-close")
                                        .ghost()
                                        .icon(IconName::Close)
                                        .accessibility_label("关闭窗口")
                                        .tooltip("关闭")
                                        .on_click(|_, window, _| window.remove_window()),
                                )
                            }),
                    ),
                )
            })
            .child(
                h_flex()
                    .items_stretch()
                    .flex_1()
                    .min_h_0()
                    .child(
                        div()
                            .relative()
                            .flex_1()
                            .min_w_0()
                            .min_h_0()
                            .child(self.player.clone())
                            .when(loading, |this| {
                                this.child(overlay(
                                    v_flex()
                                        .items_center()
                                        .gap_3()
                                        .child(Spinner::new().large())
                                        .child(
                                            div()
                                                .text_sm()
                                                .text_color(muted)
                                                .child("正在获取视频流…"),
                                        ),
                                    cx,
                                ))
                            })
                            .children(error.clone().map(|error| {
                                overlay(
                                    v_flex()
                                        .items_center()
                                        .gap_3()
                                        .child(div().text_sm().text_color(danger).child(error))
                                        .child(
                                            Button::new("retry-playback")
                                                .primary()
                                                .label("重试")
                                                .on_click({
                                                    let weak = weak.clone();
                                                    move |_, _, cx| {
                                                        let _ = weak
                                                            .update(cx, |this, cx| this.retry(cx));
                                                    }
                                                }),
                                        ),
                                    cx,
                                )
                            })),
                    )
                    .when(show_info, |this| this.child(info)),
            )
    }
}

fn overlay(content: impl IntoElement, cx: &App) -> impl IntoElement {
    let theme = cx.theme();
    div()
        .absolute()
        .size_full()
        .flex()
        .items_center()
        .justify_center()
        .child(
            div()
                .p_6()
                .rounded(theme.radius_lg)
                .border_1()
                .border_color(theme.border)
                .bg(theme.popover)
                .text_color(theme.popover_foreground)
                .child(content),
        )
}
