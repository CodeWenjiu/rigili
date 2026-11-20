use gpui_kit::assets::IconName;
use gpui_kit::component::avatar::Avatar;
use gpui_kit::component::button::*;
use gpui_kit::component::notification::Notification;
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::component::*;
use gpui_kit::prelude::*;
use gpui_kit::*;
use rigili_bilibili::banner;
use rigili_core::api_task_error_message;
use rigili_core::format_count;
use rigili_core::login as login_state;
use rigili_core::runtime::spawn_api;
use rigili_core::toast;

use super::super::compose;
use super::{CloseSpace, SpaceView};
use crate::pages::account::login;
use crate::shared::ui::profile::{profile_stat, profile_title};

impl SpaceView {
    pub(super) fn load_profile(&mut self, cx: &mut Context<Self>) {
        let Some(mid) = self.mid else {
            return;
        };
        if self.profile_loading {
            return;
        }
        let client = match self.cached_client(cx) {
            Ok(client) => client,
            Err(error) => {
                self.profile_loading = false;
                self.profile_error = Some(error);
                cx.notify();
                return;
            }
        };
        self.profile_loading = true;
        self.profile = None;
        self.profile_error = None;
        cx.notify();

        spawn_api(
            cx,
            async move { client.space_profile(mid).await },
            move |this, result, cx| {
                if this.mid != Some(mid) {
                    return;
                }
                this.profile_loading = false;
                match result {
                    Ok(profile) => {
                        this.followed = profile.followed();
                        this.profile = Some(profile);
                        this.profile_error = None;
                    }
                    Err(error) => this.profile_error = Some(api_task_error_message(error, cx)),
                }
                cx.notify();
            },
        );
    }

    fn toggle_follow(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(mid) = self.mid else {
            return;
        };
        if self.follow_pending || self.profile.is_none() {
            return;
        }
        let Some(user) = login_state::user(cx) else {
            login::open(window, cx);
            return;
        };
        if Some(user.mid) == self.mid {
            return;
        }
        let client = match self.cached_client(cx) {
            Ok(client) => client,
            Err(error) => {
                toast::push(window, cx, Notification::error(error));
                return;
            }
        };

        let follow = !self.followed;
        let handle = window.window_handle();
        self.follow_pending = true;
        cx.notify();

        spawn_api(
            cx,
            async move { client.modify_relation(mid, follow).await },
            move |this, result, cx| {
                this.follow_pending = false;
                match result {
                    Ok(()) => {
                        if this.mid == Some(mid) {
                            this.apply_follow(follow);
                        }
                        let note = Notification::success(if follow {
                            "已关注"
                        } else {
                            "已取消关注"
                        });
                        let _ = handle.update(cx, |_, window, cx| {
                            toast::push(window, cx, note);
                        });
                    }
                    Err(error) => {
                        let note = Notification::error(api_task_error_message(error, cx));
                        let _ = handle.update(cx, |_, window, cx| {
                            toast::push(window, cx, note);
                        });
                    }
                }
                cx.notify();
            },
        );
    }

    fn apply_follow(&mut self, follow: bool) {
        self.followed = follow;
        if let Some(relation) = self
            .profile
            .as_mut()
            .and_then(|profile| profile.relation.as_mut())
        {
            relation.follower = if follow {
                relation.follower.saturating_add(1)
            } else {
                relation.follower.saturating_sub(1)
            };
        }
    }

    fn open_message(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(mid) = self.mid else {
            return;
        };
        if login_state::user(cx).is_none() {
            login::open(window, cx);
            return;
        }
        let name = self
            .profile
            .as_ref()
            .map(|profile| profile.user.name.clone())
            .filter(|name| !name.is_empty())
            .unwrap_or_else(|| self.name.clone());
        compose::open(window, cx, mid, name);
    }

    pub(super) fn render_profile(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = cx.theme();
        let muted = theme.muted_foreground;
        let primary = theme.primary;
        let background = theme.background;
        let danger = theme.danger;
        let banner_bg = linear_gradient(
            120.,
            linear_color_stop(primary.opacity(0.32), 0.),
            linear_color_stop(background, 1.),
        );
        let radius = theme.radius_lg;

        let profile = self.profile.as_ref();
        let viewer = login_state::user(cx);
        let seed = viewer.as_ref().filter(|user| Some(user.mid) == self.mid);
        let name = profile
            .map(|profile| profile.user.name.as_str())
            .filter(|name| !name.is_empty())
            .or_else(|| seed.as_ref().map(|user| user.uname.as_str()))
            .unwrap_or(self.name.as_str());
        let face = profile
            .map(|profile| profile.user.face.clone())
            .or_else(|| seed.as_ref().map(|user| user.face.clone()))
            .unwrap_or_default();
        let sign = profile
            .map(|profile| profile.user.sign.clone())
            .unwrap_or_default();
        let banner_url = profile
            .map(|profile| profile.user.top_photo.as_str())
            .filter(|url| !url.is_empty());
        let level = profile
            .map(|profile| profile.user.level)
            .or_else(|| seed.as_ref().and_then(|user| user.level()))
            .unwrap_or(0);
        let official = profile
            .and_then(|profile| profile.official_title())
            .map(str::to_owned);
        let vip = profile
            .and_then(|profile| profile.vip_label())
            .map(str::to_owned)
            .or_else(|| {
                seed.as_ref()
                    .and_then(|user| user.vip_label())
                    .map(str::to_owned)
            });
        let stat_value = |value: Option<u64>| match value {
            Some(value) => format_count(value),
            None => "—".into(),
        };
        let following = stat_value(profile.and_then(|profile| profile.following()));
        let follower = stat_value(profile.and_then(|profile| profile.follower()));
        let likes = stat_value(profile.and_then(|profile| profile.likes()));
        let loading_sign = self.profile.is_none() && self.profile_loading;
        let error = self.profile_error.clone();
        let is_self = viewer
            .as_ref()
            .is_some_and(|user| Some(user.mid) == self.mid);
        let followed = self.followed;
        let follow_pending = self.follow_pending;
        let actions_ready = self.profile.is_some();

        v_flex()
            .w_full()
            .flex_shrink_0()
            .gap_2()
            .child(
                div()
                    .relative()
                    .w_full()
                    .h(px(132.))
                    .rounded(radius)
                    .overflow_hidden()
                    .bg(banner_bg)
                    .when_some(banner_url, |this, url| {
                        this.child(
                            img(banner(url, 1280))
                                .size_full()
                                .object_fit(ObjectFit::Cover),
                        )
                    })
                    .child(
                        div().absolute().top_3().left_3().child(
                            Button::new("space-back")
                                .secondary()
                                .rounded(ButtonRounded::Large)
                                .w(px(32.))
                                .h(px(32.))
                                .icon(IconName::ChevronLeft)
                                .accessibility_label("返回")
                                .on_click(cx.listener(|_, _, _, cx| cx.emit(CloseSpace))),
                        ),
                    ),
            )
            .child(
                h_flex()
                    .w_full()
                    .px_4()
                    .items_start()
                    .gap_4()
                    .child(
                        div()
                            .flex_shrink_0()
                            .mt(px(-32.))
                            .p_1()
                            .rounded_full()
                            .bg(background)
                            .child(Avatar::new().src(face).name(name.to_owned()).size(px(72.))),
                    )
                    .child(
                        v_flex()
                            .flex_1()
                            .min_w_0()
                            .gap_1()
                            .pt_2()
                            .child(profile_title(
                                name.to_owned(),
                                Some(level),
                                vip,
                                official,
                                cx,
                            ))
                            .child(
                                div()
                                    .id(SharedString::from(format!(
                                        "space-sign-{}",
                                        self.mid.unwrap_or_default()
                                    )))
                                    .h(px(18.))
                                    .truncate()
                                    .text_sm()
                                    .text_color(muted)
                                    .tooltip({
                                        let sign = sign.clone();
                                        move |window, cx| {
                                            Tooltip::new(SharedString::from(sign.clone()))
                                                .build(window, cx)
                                        }
                                    })
                                    .child(if !sign.is_empty() {
                                        sign.clone()
                                    } else if loading_sign {
                                        "正在加载…".to_owned()
                                    } else {
                                        "暂无简介".to_owned()
                                    }),
                            ),
                    )
                    .child(
                        h_flex()
                            .flex_shrink_0()
                            .items_center()
                            .gap_6()
                            .pt_3()
                            .child(
                                h_flex()
                                    .gap_8()
                                    .child(profile_stat("关注数", following, cx))
                                    .child(profile_stat("粉丝数", follower, cx))
                                    .child(profile_stat("获赞数", likes, cx)),
                            )
                            .when(!is_self, |this| {
                                this.child(
                                    h_flex()
                                        .items_center()
                                        .gap_2()
                                        .child(
                                            Button::new("space-follow")
                                                .small()
                                                .min_w(px(84.))
                                                .when(followed, |button| {
                                                    button.secondary().icon(IconName::Check)
                                                })
                                                .when(!followed, |button| {
                                                    button.primary().icon(IconName::Plus)
                                                })
                                                .label(if followed {
                                                    "已关注"
                                                } else {
                                                    "关注"
                                                })
                                                .loading(follow_pending)
                                                .disabled(follow_pending || !actions_ready)
                                                .tooltip(if followed {
                                                    "取消关注"
                                                } else {
                                                    "关注 UP 主"
                                                })
                                                .on_click(cx.listener(|this, _, window, cx| {
                                                    this.toggle_follow(window, cx)
                                                })),
                                        )
                                        .child(
                                            Button::new("space-message")
                                                .small()
                                                .primary()
                                                .outline()
                                                .label("发消息")
                                                .tooltip("发送私信")
                                                .on_click(cx.listener(|this, _, window, cx| {
                                                    this.open_message(window, cx)
                                                })),
                                        ),
                                )
                            }),
                    ),
            )
            .when_some(error, |this, error| {
                this.child(
                    h_flex()
                        .items_center()
                        .gap_3()
                        .child(div().text_sm().text_color(danger).child(error))
                        .child(
                            Button::new("space-profile-retry")
                                .small()
                                .label("重试")
                                .on_click(cx.listener(|this, _, _, cx| this.load_profile(cx))),
                        ),
                )
            })
            .into_any_element()
    }
}
