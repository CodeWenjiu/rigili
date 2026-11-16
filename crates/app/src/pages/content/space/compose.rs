use gpui_kit::component::button::*;
use gpui_kit::component::input::{InputEvent, Textarea, TextareaState};
use gpui_kit::component::notification::Notification;
use gpui_kit::component::*;
use gpui_kit::prelude::*;
use gpui_kit::*;
use rigili_core::client;
use rigili_core::runtime::spawn_api;
use rigili_core::{api_error_message, api_task_error_message};

pub(super) fn open(window: &mut Window, cx: &mut App, receiver: u64, name: String) {
    let view = cx.new(|cx| ComposeMessage::new(window, cx, receiver, name));
    window.open_dialog(cx, move |dialog, _, _| {
        let send = view.clone();
        dialog
            .w(px(420.))
            .title("发消息")
            .on_ok(move |_, _, cx| {
                send.update(cx, |this, cx| this.submit(cx));
                cx.stop_propagation();
                false
            })
            .child(view.clone())
    });
}

struct ComposeMessage {
    input: Entity<TextareaState>,
    receiver: u64,
    name: String,
    sending: bool,
    error: Option<String>,
    window: AnyWindowHandle,
    focus_once: bool,
    _sub: Subscription,
}

impl ComposeMessage {
    fn new(window: &mut Window, cx: &mut Context<Self>, receiver: u64, name: String) -> Self {
        let input = cx.new(|cx| {
            TextareaState::new(window, cx)
                .auto_grow(3, 8)
                .submit_on_enter(true)
                .placeholder("发个消息聊聊吧~")
        });
        let sub = cx.subscribe_in(&input, window, |this, _, event: &InputEvent, _, cx| {
            if let InputEvent::PressEnter { shift: false, .. } = event {
                this.submit(cx);
            }
        });

        Self {
            input,
            receiver,
            name,
            sending: false,
            error: None,
            window: window.window_handle(),
            focus_once: true,
            _sub: sub,
        }
    }

    fn submit(&mut self, cx: &mut Context<Self>) {
        if self.sending {
            return;
        }
        let content = self.input.read(cx).value().trim().to_owned();
        if content.is_empty() {
            self.error = Some("请输入消息内容".into());
            cx.notify();
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

        let receiver = self.receiver;
        let window = self.window;
        self.sending = true;
        self.error = None;
        cx.notify();

        spawn_api(
            cx,
            async move { client.send_message(receiver, &content).await },
            move |this, result, cx| match result {
                Ok(()) => {
                    let _ = window.update(cx, |_, window, cx| {
                        window.close_dialog(cx);
                        window.push_notification(Notification::success("消息已发送"), cx);
                    });
                }
                Err(error) => {
                    this.sending = false;
                    this.error = Some(api_task_error_message(error, cx));
                    cx.notify();
                }
            },
        );
    }
}

impl Render for ComposeMessage {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let muted = cx.theme().muted_foreground;
        let danger = cx.theme().danger;

        if self.focus_once {
            self.focus_once = false;
            let input = self.input.clone();
            window.defer(cx, move |window, cx| {
                input.update(cx, |input, cx| input.focus(window, cx));
            });
        }

        v_flex()
            .gap_3()
            .child(
                div()
                    .text_sm()
                    .text_color(muted)
                    .child(format!("发送给 {}", self.name)),
            )
            .child(
                div()
                    .w_full()
                    .key_context(rigili_core::TEXT_INPUT_CONTEXT)
                    .child(Textarea::new(&self.input).w_full()),
            )
            .when_some(self.error.clone(), |this, error| {
                this.child(div().text_sm().text_color(danger).child(error))
            })
            .child(
                h_flex()
                    .justify_between()
                    .items_center()
                    .gap_3()
                    .child(
                        div()
                            .text_xs()
                            .text_color(muted)
                            .child("Enter 发送，Shift+Enter 换行"),
                    )
                    .child(
                        h_flex()
                            .flex_shrink_0()
                            .gap_2()
                            .child(
                                Button::new("space-message-cancel")
                                    .small()
                                    .label("取消")
                                    .on_click(
                                        cx.listener(|_, _, window, cx| window.close_dialog(cx)),
                                    ),
                            )
                            .child(
                                Button::new("space-message-send")
                                    .small()
                                    .primary()
                                    .disabled(self.sending)
                                    .label(if self.sending {
                                        "发送中…"
                                    } else {
                                        "发送"
                                    })
                                    .on_click(cx.listener(|this, _, _, cx| this.submit(cx))),
                            ),
                    ),
            )
    }
}
