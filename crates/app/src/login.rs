use std::{sync::Arc, time::Duration};

use gpui_kit::component::button::*;
use gpui_kit::component::spinner::Spinner;
use gpui_kit::component::*;
use gpui_kit::prelude::*;
use gpui_kit::*;
use rigili_bilibili::{Nav, api::login::cookies_from_login_url};
use image::RgbaImage;
use qrcodegen::{QrCode, QrCodeEcc};

use crate::backend::{self, Backend};
use crate::platform::runtime::Tokio;

const POLL_INTERVAL: Duration = Duration::from_secs(2);
const QR_SCALE: u32 = 6;
const QR_QUIET_ZONE: u32 = 4;
const QR_SIZE: f32 = 200.;
const QR_DARK: [u8; 4] = [0x18, 0x19, 0x1c, 0xff];
const QR_LIGHT: [u8; 4] = [0xff, 0xff, 0xff, 0xff];

enum Phase {
    Starting,
    Waiting,
    Scanned,
    Expired,
    Failed(String),
    Done,
}

#[derive(Default)]
pub(crate) struct LoginState {
    pub(crate) user: Option<Nav>,
}

impl Global for LoginState {}

pub(crate) fn user(cx: &App) -> Option<Nav> {
    cx.global::<LoginState>().user.clone()
}

pub(crate) fn set_user(user: Option<Nav>, cx: &mut App) {
    cx.set_global(LoginState { user });
    cx.refresh_windows();
}

pub(crate) fn expire(cx: &mut App) {
    if user(cx).is_some() {
        set_user(None, cx);
    }
}

struct LoginView {
    backend: Option<Backend>,
    phase: Phase,
    qr: Option<Arc<RenderImage>>,
    _task: Option<Task<()>>,
}

pub(crate) fn open(window: &mut Window, cx: &mut App) {
    let view = cx.new(|cx| LoginView::new(window, cx));
    window.open_dialog(cx, move |dialog, _, _| {
        dialog.w(px(280.)).title("登录").child(view.clone())
    });
}

pub(crate) fn restore(_window: &mut Window, cx: &mut App) {
    let Ok(backend) = backend::get(cx) else {
        return;
    };
    if !backend.has_session() {
        return;
    }
    cx.spawn(async move |cx| {
        let nav = request(Tokio::spawn(&*cx, async move { backend.nav().await }).await);
        if let Ok(nav) = nav
            && nav.is_login
        {
            cx.update(|cx| set_user(Some(nav), cx));
        }
    })
    .detach();
}

impl LoginView {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let (backend, phase) = match backend::get(cx) {
            Ok(backend) => (Some(backend), Phase::Starting),
            Err(error) => (None, Phase::Failed(error)),
        };
        let mut view = Self {
            backend,
            phase,
            qr: None,
            _task: None,
        };
        if view.backend.is_some() {
            view.start(window, cx);
        }
        view
    }

    fn start(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(backend) = self.backend.clone() else {
            return;
        };
        self.phase = Phase::Starting;
        self.qr = None;
        cx.notify();

        self._task = Some(cx.spawn_in(window, async move |weak, cx| {
            let generated = request(
                Tokio::spawn(&*cx, {
                    let backend = backend.clone();
                    async move { backend.qr_generate().await }
                })
                .await,
            );
            let qr = match generated {
                Ok(qr) => qr,
                Err(message) => {
                    let _ = weak.update_in(cx, |this, _, cx| this.fail(message, cx));
                    return;
                }
            };

            let image = qr_image(&qr.url);
            if weak
                .update_in(cx, |this, _, cx| {
                    this.qr = image;
                    this.phase = Phase::Waiting;
                    cx.notify();
                })
                .is_err()
            {
                return;
            }

            loop {
                cx.background_executor().timer(POLL_INTERVAL).await;
                let polled = Tokio::spawn(&*cx, {
                    let backend = backend.clone();
                    let key = qr.qrcode_key.clone();
                    async move { backend.qr_poll(&key).await }
                })
                .await;

                let poll = match request(polled) {
                    Ok(poll) => poll,
                    Err(message) => {
                        let _ = weak.update_in(cx, |this, _, cx| this.fail(message, cx));
                        return;
                    }
                };

                match poll.code {
                    0 => {
                        let mut cookies = cookies_from_login_url(&poll.url);
                        cookies.extend(backend.cookie_jar_values());
                        if !cookies.contains_key("SESSDATA") {
                            eprintln!(
                                "login: SESSDATA missing, cookies = {:?}",
                                cookies.keys().collect::<Vec<_>>()
                            );
                            let _ = weak.update_in(cx, |this, _, cx| {
                                this.fail("登录未完成，请重新扫码".into(), cx)
                            });
                            return;
                        }
                        backend.apply_cookies(cookies);
                        let saved = backend.save_session();
                        let nav = request(
                            Tokio::spawn(&*cx, {
                                let backend = backend.clone();
                                async move { backend.nav().await }
                            })
                            .await,
                        );
                        let _ = weak.update_in(cx, |this, window, cx| {
                            this.finish(nav, saved.err(), window, cx)
                        });
                        return;
                    }
                    86101 => {}
                    86090 => {
                        let _ = weak.update_in(cx, |this, _, cx| {
                            if matches!(this.phase, Phase::Waiting) {
                                this.phase = Phase::Scanned;
                                cx.notify();
                            }
                        });
                    }
                    86038 => {
                        let _ = weak.update_in(cx, |this, _, cx| {
                            this.phase = Phase::Expired;
                            cx.notify();
                        });
                        return;
                    }
                    code => {
                        let message = format!("登录失败（{code}）：{}", poll.message);
                        let _ = weak.update_in(cx, |this, _, cx| this.fail(message, cx));
                        return;
                    }
                }
            }
        }));
    }

    fn fail(&mut self, message: String, cx: &mut Context<Self>) {
        self.phase = Phase::Failed(message);
        cx.notify();
    }

    fn finish(
        &mut self,
        nav: Result<Nav, String>,
        save_error: Option<rigili_bilibili::SessionError>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match nav {
            Ok(nav) if nav.is_login => {
                self.phase = Phase::Done;
                set_user(Some(nav), cx);
            }
            Ok(_) => self.fail("登录状态未生效，请重试".into(), cx),
            Err(message) => self.fail(message, cx),
        }
        if let Some(error) = save_error {
            eprintln!("failed to save session: {error}");
        }
        cx.notify();
        window.close_dialog(cx);
    }

    fn render_qr(&self, cx: &mut Context<Self>) -> AnyElement {
        match &self.qr {
            Some(image) => img(image.clone())
                .w(px(QR_SIZE))
                .h(px(QR_SIZE))
                .rounded(cx.theme().radius)
                .into_any_element(),
            None => div()
                .w(px(QR_SIZE))
                .h(px(QR_SIZE))
                .rounded(cx.theme().radius)
                .bg(cx.theme().muted)
                .flex()
                .items_center()
                .justify_center()
                .child(Spinner::new().large())
                .into_any_element(),
        }
    }
}

impl Render for LoginView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let muted = cx.theme().muted_foreground;
        let danger = cx.theme().danger;
        let weak = cx.entity().downgrade();
        let retryable = matches!(self.phase, Phase::Expired | Phase::Failed(_));

        let status = match &self.phase {
            Phase::Starting => div().text_sm().text_color(muted).child("正在获取二维码…"),
            Phase::Waiting => div()
                .text_sm()
                .text_color(muted)
                .child("使用哔哩哔哩 App 扫码登录"),
            Phase::Scanned => div()
                .text_sm()
                .text_color(muted)
                .child("扫码成功，请在手机上确认"),
            Phase::Expired => div()
                .text_sm()
                .text_color(muted)
                .child("二维码已过期，请重新获取"),
            Phase::Failed(message) => div().text_sm().text_color(danger).child(message.clone()),
            Phase::Done => div().text_sm().text_color(muted).child("登录成功"),
        };

        v_flex()
            .items_center()
            .gap_4()
            .px_4()
            .pb_4()
            .child(self.render_qr(cx))
            .child(status)
            .when(retryable, |this| {
                this.child(
                    Button::new("reload-qr")
                        .primary()
                        .label("重新获取")
                        .on_click(move |_, window, cx| {
                            let _ = weak.update(cx, |this, cx| this.start(window, cx));
                        }),
                )
            })
    }
}

fn request<T>(
    result: Result<Result<T, rigili_bilibili::ApiError>, tokio::task::JoinError>,
) -> Result<T, String> {
    match result {
        Ok(Ok(value)) => Ok(value),
        Ok(Err(error)) => Err(error.to_string()),
        Err(error) => Err(format!("加载任务失败：{error}")),
    }
}

fn qr_image(url: &str) -> Option<Arc<RenderImage>> {
    let pixels = qr_pixels(url)?;
    Some(Arc::new(RenderImage::new([image::Frame::new(pixels)])))
}

fn qr_pixels(url: &str) -> Option<RgbaImage> {
    let code = QrCode::encode_text(url, QrCodeEcc::Medium).ok()?;
    let modules = code.size() as u32;
    let side = (modules + QR_QUIET_ZONE * 2) * QR_SCALE;
    let mut pixels = RgbaImage::new(side, side);

    for y in 0..side {
        for x in 0..side {
            let px = x / QR_SCALE;
            let py = y / QR_SCALE;
            let dark = px >= QR_QUIET_ZONE
                && py >= QR_QUIET_ZONE
                && px < modules + QR_QUIET_ZONE
                && py < modules + QR_QUIET_ZONE
                && code.get_module((px - QR_QUIET_ZONE) as i32, (py - QR_QUIET_ZONE) as i32);
            let color = if dark { QR_DARK } else { QR_LIGHT };
            pixels.put_pixel(x, y, image::Rgba(color));
        }
    }

    Some(pixels)
}

#[cfg(test)]
mod tests {
    use super::qr_pixels;

    #[test]
    fn renders_a_square_qr_image() {
        let pixels =
            qr_pixels("https://account.bilibili.com/h5/account-h5/auth/scan-web?qrcode_key=abc")
                .expect("qr pixels");
        assert_eq!(pixels.width(), pixels.height());
        assert!(pixels.width() > 100);
    }
}
