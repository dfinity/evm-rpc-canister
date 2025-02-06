use candid::{CandidType, Principal};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, CandidType, Deserialize)]
pub struct InstallArgs {
    pub demo: Option<bool>,
    #[serde(rename = "manageApiKeys")]
    pub manage_api_keys: Option<Vec<Principal>>,
    #[serde(rename = "logFilter")]
    pub log_filter: Option<LogFilter>,
    #[serde(rename = "overrideProvider")]
    pub override_provider: Option<OverrideProvider>,
    #[serde(rename = "nodesInSubnet")]
    pub nodes_in_subnet: Option<u32>,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Serialize, Deserialize)]
pub enum LogFilter {
    ShowAll,
    HideAll,
    ShowPattern(RegexString),
    HidePattern(RegexString),
}

#[derive(Clone, Debug, Default, PartialEq, Eq, CandidType, Serialize, Deserialize)]
pub struct OverrideProvider {
    #[serde(rename = "overrideUrl")]
    pub override_url: Option<RegexSubstitution>,
}

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Serialize, Deserialize)]
pub struct RegexString(pub String);

#[derive(Clone, Debug, PartialEq, Eq, CandidType, Serialize, Deserialize)]
pub struct RegexSubstitution {
    pub pattern: RegexString,
    pub replacement: String,
}
