use std::collections::BTreeMap;

use serde::Deserialize;

use super::SearchUser;

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(from = "RawSearchAll")]
pub struct SearchAll {
    pub users: Vec<SearchUser>,
    pub pageinfo: BTreeMap<String, SearchPageInfo>,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SearchPageInfo {
    #[serde(default)]
    pub num_results: u64,
}

#[derive(Deserialize)]
struct RawSearchAll {
    #[serde(default, rename = "result")]
    groups: Vec<RawSearchGroup>,
    #[serde(default)]
    pageinfo: BTreeMap<String, SearchPageInfo>,
}

#[derive(Deserialize)]
struct RawSearchGroup {
    #[serde(default)]
    result_type: String,
    #[serde(default)]
    data: Vec<serde_json::Value>,
}

impl From<RawSearchAll> for SearchAll {
    fn from(raw: RawSearchAll) -> Self {
        let mut users = Vec::new();
        for group in raw.groups {
            if group.result_type != "bili_user" {
                continue;
            }
            users.extend(
                group
                    .data
                    .into_iter()
                    .filter_map(|data| serde_json::from_value(data).ok()),
            );
        }

        Self {
            users,
            pageinfo: raw.pageinfo,
        }
    }
}
