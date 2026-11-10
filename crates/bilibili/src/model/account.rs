use serde::Deserialize;

#[derive(Clone, Debug, Deserialize, PartialEq)]
pub struct QrCode {
    pub url: String,
    pub qrcode_key: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
pub struct QrPoll {
    pub code: u32,
    #[serde(default)]
    pub message: String,
    #[serde(default)]
    pub url: String,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Nav {
    #[serde(default)]
    pub is_login: bool,
    #[serde(default)]
    pub uname: String,
    #[serde(default)]
    pub face: String,
    #[serde(default)]
    pub mid: u64,
    #[serde(default)]
    pub money: f64,
    #[serde(default, rename = "level_info")]
    pub level_info: Option<NavLevel>,
    #[serde(default)]
    pub vip: Option<NavVip>,
    #[serde(default)]
    pub wallet: Option<NavWallet>,
}

impl Nav {
    pub fn level(&self) -> Option<u32> {
        self.level_info
            .as_ref()
            .map(|level| level.current_level)
            .filter(|level| *level > 0)
    }

    pub fn vip_label(&self) -> Option<&str> {
        let vip = self.vip.as_ref()?;
        let label = vip.label.as_ref()?;
        (vip.status > 0 && !label.text.is_empty()).then_some(label.text.as_str())
    }

    pub fn coins(&self) -> Option<f64> {
        (self.money > 0.0).then_some(self.money)
    }

    pub fn bcoins(&self) -> Option<f64> {
        let wallet = self.wallet.as_ref()?;
        (wallet.bcoin_balance > 0.0).then_some(wallet.bcoin_balance)
    }
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
pub struct NavLevel {
    #[serde(default)]
    pub current_level: u32,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
pub struct NavVip {
    #[serde(default)]
    pub status: u32,
    #[serde(default)]
    pub label: Option<NavVipLabel>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
pub struct NavVipLabel {
    #[serde(default)]
    pub text: String,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
pub struct NavWallet {
    #[serde(default)]
    pub bcoin_balance: f64,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
pub struct NavStat {
    #[serde(default)]
    pub dynamic_count: u64,
    #[serde(default)]
    pub following: u64,
    #[serde(default)]
    pub follower: u64,
}

#[cfg(test)]
mod tests {
    use super::Nav;

    #[test]
    fn parses_nav_level_vip_and_wallet() {
        let nav: Nav = serde_json::from_str(
            r#"{
                "isLogin": true,
                "uname": "测试用户",
                "face": "https://example.com/face.jpg",
                "mid": 42,
                "money": 1.7,
                "level_info": {"current_level": 6, "current_exp": 0, "next_exp": "28800"},
                "vip": {
                    "status": 1,
                    "label": {"text": "年度大会员"}
                },
                "wallet": {"bcoin_balance": 0.0}
            }"#,
        )
        .expect("nav");

        assert!(nav.is_login);
        assert_eq!(nav.mid, 42);
        assert_eq!(nav.level(), Some(6));
        assert_eq!(nav.vip_label(), Some("年度大会员"));
        assert_eq!(nav.coins(), Some(1.7));
        assert_eq!(nav.bcoins(), None);
    }

    #[test]
    fn hides_level_and_vip_when_not_a_member() {
        let nav: Nav =
            serde_json::from_str(r#"{"level_info": {"current_level": 0}, "vip": {"status": 0}}"#)
                .expect("nav");
        assert_eq!(nav.level(), None);
        assert_eq!(nav.vip_label(), None);
        assert_eq!(nav.coins(), None);
    }
}
