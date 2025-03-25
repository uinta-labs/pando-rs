use serde::{Deserialize, Serialize};

// I want to allow choices in how identifiers are generated,
// but I'm skeptical that doing this on the device is the best approach.
// Being server-authoritative is a good way to ensure that identifiers are unique.
// Only downside is that before registration, we need to have a way to identify the device.
// Leaning towards allowing a static identifier to be set in the config file, with
// logic for either keeping this identifier or generating a new one residing in the server.
// #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
// pub enum ConfigJsonInitName {
//     #[serde(rename = "set")]
//     Set(String),
//     #[serde(rename = "prefix")]
//     Prefix(String),
//     #[serde(rename = "random")]
//     Random,
//     #[serde(rename = "remoteAssigned")]
//     RemoteAssigned,
// }

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConfigJsonInit {
    // #[serde(rename = "name")]
    // pub name: Option<ConfigJsonInitName>,

    #[serde(rename = "nameMaxLength")]
    pub name_max_length: Option<usize>,

    #[serde(rename = "provisioningToken")]
    pub provisioning_token: Option<String>,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConfigJson {
    #[serde(rename = "developmentMode")]
    pub development_mode: Option<bool>,

    #[serde(rename = "deviceType")]
    pub device_type: Option<String>,

    #[serde(rename = "persistentLogging")]
    pub persistent_logging: Option<bool>,

    #[serde(rename = "uuid")]
    pub uuid: Option<String>,

    #[serde(rename = "apiToken")]
    pub api_token: Option<String>,

    #[serde(rename = "apiEndpoint")]
    pub api_endpoint: Option<String>,

    #[serde(rename = "init")]
    pub init: Option<ConfigJsonInit>
}


impl ConfigJson {
    pub fn new() -> Self {
        ConfigJson::default()
    }

    pub fn read_from(path: &str) -> Result<Self, std::io::Error> {
        let file = std::fs::File::open(path)?;
        let reader = std::io::BufReader::new(file);
        let config = serde_json::from_reader(reader)?;
        Ok(config)
    }
}
