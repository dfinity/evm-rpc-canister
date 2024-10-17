use candid::{CandidType, Principal};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, CandidType, Deserialize)]
pub struct InstallArgs {
    pub demo: Option<bool>,
    #[serde(rename = "manageApiKeys")]
    pub manage_api_keys: Option<Vec<Principal>>,
    #[serde(rename = "logFilter")]
    pub log_filter: Option<LogFilter>,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Serialize, Deserialize)]
pub enum LogFilter {
    ShowAll,
    HideAll,
    ShowPattern(RegexString),
    HidePattern(RegexString),
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Serialize, Deserialize)]
pub struct RegexString(pub String);
