use gpui_kit::component::notification::Notification;
use gpui_kit::prelude::*;
use gpui_kit::*;
use rigili_bilibili::{ApiError, Client};
use rigili_core::client;
use rigili_core::runtime::{ApiTaskError, spawn_api};
use rigili_core::toast;
use rigili_core::{api_error_message, api_task_error_message};

use super::InfoPanel;

/// What a one-tap triple still has to do for the current interactions.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct TriplePlan {
    pub(super) like: bool,
    pub(super) coins: u32,
    pub(super) favorite: bool,
}

impl TriplePlan {
    pub(super) fn new(liked: bool, coins: u32, favorited: bool) -> Self {
        Self {
            like: !liked,
            coins: 2u32.saturating_sub(coins.min(2)),
            favorite: !favorited,
        }
    }

    pub(super) fn is_empty(&self) -> bool {
        !self.like && self.coins == 0 && !self.favorite
    }
}

#[derive(Default)]
struct TripleReport {
    liked: bool,
    coins: u32,
    favorited: bool,
    error: Option<TripleError>,
}

enum TripleError {
    Api(ApiError),
    MissingFolder,
}

impl InfoPanel {
    pub(super) fn triple_plan(&self) -> TriplePlan {
        TriplePlan::new(self.like.active, self.coin.given, self.favorite.active)
    }

    pub(super) fn triple(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(details) = self.details.clone() else {
            return;
        };
        let plan = self.triple_plan();
        if self.triple_busy || plan.is_empty() {
            return;
        }
        let client = match client::get(cx) {
            Ok(client) => client,
            Err(error) => {
                let message = api_error_message(error);
                toast::push(window, cx, Notification::error(message));
                return;
            }
        };

        let aid = details.aid;
        let bvid = details.bvid.to_string();
        let window_handle = window.window_handle();
        self.triple_busy = true;
        cx.notify();

        spawn_api(
            cx,
            async move { Ok(run(&client, aid, &bvid, plan).await) },
            move |this, result, cx| {
                let (report, task_error) = match result {
                    Ok(report) => (report, None),
                    Err(error) => (TripleReport::default(), Some(error)),
                };
                if this
                    .details
                    .as_ref()
                    .is_some_and(|details| details.aid == aid)
                {
                    this.finish_triple(&report, cx);
                }
                let message = match (report.error, task_error) {
                    (Some(TripleError::Api(error)), _) => {
                        Some(api_task_error_message(ApiTaskError::Api(error), cx))
                    }
                    (Some(TripleError::MissingFolder), _) => {
                        Some("找不到可用的默认收藏夹".to_owned())
                    }
                    (None, Some(error)) => Some(api_task_error_message(error, cx)),
                    (None, None) => None,
                };
                if let Some(message) = message {
                    let _ = window_handle.update(cx, |_, window, cx| {
                        toast::push(window, cx, Notification::error(message));
                    });
                }
            },
        );
    }

    fn finish_triple(&mut self, report: &TripleReport, cx: &mut Context<Self>) {
        if report.liked {
            self.like.apply(true);
        }
        if report.coins > 0 {
            self.coin.grant(report.coins);
        }
        if report.favorited {
            self.favorite.apply(true);
        }
        self.triple_busy = false;
        cx.notify();
    }
}

async fn run(client: &Client, aid: u64, bvid: &str, plan: TriplePlan) -> TripleReport {
    let mut report = TripleReport::default();
    if plan.like {
        match client.set_like(aid, true).await {
            Ok(()) => report.liked = true,
            // A like that fails is a session or network failure, which the
            // remaining steps would hit too.
            Err(error) => {
                report.error = Some(TripleError::Api(error));
                return report;
            }
        }
    }
    if plan.favorite {
        match favorite_default(client, aid).await {
            Ok(()) => report.favorited = true,
            Err(error) => report.error = Some(error),
        }
    }
    // Coins are the step Bilibili is most likely to refuse (balance, daily cap),
    // so they go last: a refusal still leaves the like and the favourite behind.
    if plan.coins > 0 {
        match client.add_coin(aid, bvid, plan.coins, false).await {
            Ok(()) => report.coins = plan.coins,
            Err(error) => {
                report.error.get_or_insert(TripleError::Api(error));
            }
        }
    }
    report
}

async fn favorite_default(client: &Client, aid: u64) -> Result<(), TripleError> {
    let nav = client.nav().await.map_err(TripleError::Api)?;
    let folders = client
        .fav_folders_with_state(nav.mid, aid)
        .await
        .map_err(TripleError::Api)?;
    let folder = folders
        .into_items()
        .into_iter()
        .find(|folder| folder.is_default() && folder.is_valid())
        .ok_or(TripleError::MissingFolder)?;
    client
        .set_favorite(aid, folder.id, true)
        .await
        .map_err(TripleError::Api)
}

#[cfg(test)]
mod tests {
    use super::TriplePlan;

    #[test]
    fn plans_only_the_parts_that_are_missing() {
        assert_eq!(
            TriplePlan::new(false, 0, false),
            TriplePlan {
                like: true,
                coins: 2,
                favorite: true,
            }
        );
        assert_eq!(
            TriplePlan::new(true, 1, true),
            TriplePlan {
                like: false,
                coins: 1,
                favorite: false,
            }
        );
        assert!(TriplePlan::new(true, 2, true).is_empty());
    }

    #[test]
    fn never_asks_for_more_than_two_coins() {
        assert_eq!(TriplePlan::new(true, 0, true).coins, 2);
        assert_eq!(TriplePlan::new(true, 1, true).coins, 1);
        assert_eq!(TriplePlan::new(true, 2, true).coins, 0);
        assert_eq!(TriplePlan::new(true, 5, true).coins, 0);
    }
}
