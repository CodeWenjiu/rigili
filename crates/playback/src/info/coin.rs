use gpui_kit::component::button::*;
use gpui_kit::component::checkbox::Checkbox;
use gpui_kit::component::radio::Radio;
use gpui_kit::component::*;
use gpui_kit::prelude::*;
use gpui_kit::*;

use rigili_core::client;
use rigili_core::runtime::spawn_api;
use rigili_core::{api_error_message, api_task_error_message};

use super::InfoPanel;

const MAX_COINS: u32 = 2;

pub(super) struct CoinDialog {
    aid: u64,
    panel: Entity<InfoPanel>,
    window: AnyWindowHandle,
    given: u32,
    count: u32,
    liked: bool,
    also_like: bool,
    submitting: bool,
    error: Option<String>,
}

impl CoinDialog {
    pub(super) fn new(
        window: &Window,
        aid: u64,
        panel: Entity<InfoPanel>,
        given: u32,
        liked: bool,
    ) -> Self {
        Self {
            aid,
            panel,
            window: window.window_handle(),
            given: given.min(MAX_COINS),
            count: 1,
            liked,
            also_like: !liked,
            submitting: false,
            error: None,
        }
    }

    fn remaining(&self) -> u32 {
        MAX_COINS.saturating_sub(self.given)
    }

    fn submit(&mut self, cx: &mut Context<Self>) {
        let remaining = self.remaining();
        if self.submitting || remaining == 0 {
            return;
        }
        let client = match client::get(cx) {
            Ok(client) => client,
            Err(error) => {
                self.error = Some(api_error_message(error));
                cx.notify();
                return;
            }
        };
        let aid = self.aid;
        let count = self.count.min(remaining);
        let also_like = self.also_like && !self.liked;
        let panel = self.panel.clone();
        let window = self.window;
        self.submitting = true;
        self.error = None;
        cx.notify();

        spawn_api(
            cx,
            async move { client.add_coin(aid, count, also_like).await },
            move |this, result, cx| {
                this.submitting = false;
                match result {
                    Ok(()) => {
                        panel.update(cx, |panel, cx| {
                            panel.grant_coins(count, cx);
                            if also_like {
                                panel.set_liked(true, cx);
                            }
                        });
                        let _ = window.update(cx, |_, window, cx| window.close_dialog(cx));
                    }
                    Err(error) => {
                        this.error = Some(api_task_error_message(error, cx));
                        cx.notify();
                    }
                }
            },
        );
    }
}

impl Render for CoinDialog {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let muted = cx.theme().muted_foreground;
        let danger = cx.theme().danger;
        let remaining = self.remaining();

        v_flex()
            .gap_3()
            .when(self.given > 0, |this| {
                this.child(
                    div()
                        .text_xs()
                        .text_color(muted)
                        .child(format!("已投 {} 枚，还可投 {} 枚", self.given, remaining)),
                )
            })
            .child(v_flex().gap_2().children((1..=remaining).map(|count| {
                Radio::new(("coin-count", count as u64))
                    .small()
                    .label(format!("投 {count} 枚硬币"))
                    .checked(self.count == count)
                    .disabled(self.submitting)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.count = count;
                        cx.notify();
                    }))
            })))
            .when(!self.liked, |this| {
                this.child(
                    Checkbox::new("coin-also-like")
                        .small()
                        .label("同时点赞")
                        .checked(self.also_like)
                        .disabled(self.submitting)
                        .on_change(cx.listener(|this, checked, _, cx| {
                            this.also_like = *checked;
                            cx.notify();
                        })),
                )
            })
            .when_some(self.error.clone(), |this, error| {
                this.child(div().text_sm().text_color(danger).child(error))
            })
            .child(
                h_flex().justify_end().child(
                    Button::new("coin-submit")
                        .primary()
                        .small()
                        .disabled(self.submitting || remaining == 0)
                        .label(if self.submitting {
                            "投币中…"
                        } else {
                            "投币"
                        })
                        .on_click(cx.listener(|this, _, _, cx| this.submit(cx))),
                ),
            )
    }
}
