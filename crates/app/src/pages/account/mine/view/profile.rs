use gpui_kit::assets::IconName;
use gpui_kit::component::avatar::Avatar;
use gpui_kit::component::button::*;
use gpui_kit::component::*;
use gpui_kit::prelude::*;
use gpui_kit::*;
use rigili_core::{OpenSpace, format_count};

use super::MineView;
use crate::shared::ui::profile::{profile_stat, profile_title};

impl MineView {
    pub(super) fn render_profile(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = cx.theme();
        let muted = theme.muted_foreground;

        let Some(user) = rigili_core::login::user(cx) else {
            return crate::pages::account::login::Prompt::new(
                "mine",
                IconName::CircleUser,
                "登录后查看历史记录、收藏和稍后再看",
            )
            .title("未登录")
            .render(cx);
        };

        let stat = self.stat.as_ref();
        let stat_value = |value: Option<u64>| match value {
            Some(value) => format_count(value),
            None => "—".into(),
        };
        let dynamic = stat_value(stat.map(|stat| stat.dynamic_count));
        let following = stat_value(stat.map(|stat| stat.following));
        let follower = stat_value(stat.map(|stat| stat.follower));
        let balance = format!(
            "B币: {}　硬币: {}",
            format_balance(user.bcoins().unwrap_or(0.)),
            format_balance(user.coins().unwrap_or(0.))
        );
        let mid = user.mid;
        let space_name = user.uname.clone();

        h_flex()
            .w_full()
            .p_5()
            .gap_5()
            .items_center()
            .rounded(theme.radius_lg)
            .bg(theme.muted.opacity(0.4))
            .child(
                Avatar::new()
                    .src(user.face.clone())
                    .name(user.uname.clone())
                    .size(px(64.)),
            )
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .gap_2()
                    .child(profile_title(
                        user.uname.clone(),
                        user.level(),
                        user.vip_label().map(str::to_owned),
                        None,
                        cx,
                    ))
                    .child(div().text_xs().text_color(muted).child(balance)),
            )
            .child(
                h_flex()
                    .flex_shrink_0()
                    .gap_6()
                    .child(profile_stat("动态", dynamic, cx))
                    .child(profile_stat("关注", following, cx))
                    .child(profile_stat("粉丝", follower, cx)),
            )
            .child(
                Button::new("mine-open-space")
                    .text()
                    .small()
                    .text_color(muted)
                    .label("个人空间 >")
                    .on_click(cx.listener(move |_, _, _, cx| {
                        cx.emit(OpenSpace {
                            mid,
                            name: space_name.clone(),
                        });
                    })),
            )
            .into_any_element()
    }
}

fn format_balance(value: f64) -> String {
    if (value.fract()).abs() < 0.05 {
        format!("{}", value.round() as u64)
    } else {
        format!("{value:.1}")
    }
}
