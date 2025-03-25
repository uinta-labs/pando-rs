use anyhow::Result;

pub struct Temperature {
    zone: String,
}

#[cfg(target_os = "linux")]
impl Temperature {
    /// /sys/devices/virtual/thermal/thermal_zone0/temp
    /// /sys/devices/virtual/thermal/thermal_zone1/temp
    pub fn new(zone: String) -> Self {
        Self { zone }
    }

    pub async fn get_temperature(&self) -> Result<f64> {
        let path = format!("/sys/devices/virtual/thermal/{}/temp", self.zone);
        let file = tokio::fs::read_to_string(path).await?;
        let temp = file.trim().parse::<f64>()?;
        Ok(temp / 1000.0)
    }

    pub async fn get_zone_type(&self) -> Result<String> {
        let path = format!("/sys/devices/virtual/thermal/{}/type", self.zone);
        let file = tokio::fs::read_to_string(path).await?;
        Ok(file.trim().to_string())
    }
}

#[cfg(target_os = "linux")]
pub async fn list_zones() -> Result<Vec<String>> {
    let linux_path = "/sys/devices/virtual/thermal";
    let mut dir = tokio::fs::read_dir(linux_path).await?;
    let mut files = Vec::new();

    while let Some(child) = dir.next_entry().await? {
        if child.metadata().await?.is_dir() {
            files.push(child.file_name().to_str().unwrap().to_string());
        }
    }

    Ok(files)
}

#[cfg(not(target_os = "linux"))]
impl Temperature {
    pub fn new(zone: String) -> Self {
        Self { zone }
    }

    pub async fn get_temperature(&self) -> Result<f64> {
        Ok(55.5)
    }

    #[allow(dead_code)]
    pub async fn get_zone_type(&self) -> Result<String> {
        Ok(self.zone.clone())
    }
}

#[cfg(not(target_os = "linux"))]
pub async fn list_zones() -> Result<Vec<String>> {
    Ok(vec!["fake".to_string()])
}
