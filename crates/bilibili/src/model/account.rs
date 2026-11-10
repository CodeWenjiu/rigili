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
}
