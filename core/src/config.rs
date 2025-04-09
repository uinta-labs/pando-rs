use serde::{de::DeserializeOwned, Serialize};
use std::env;
use std::fs;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq)]
pub enum ConfigMode {
    #[allow(dead_code)]
    System,
    #[allow(dead_code)]
    User,
    #[allow(dead_code)]
    Path(PathBuf),
}

#[derive(Debug)]
pub struct Config<T> {
    mode: ConfigMode,
    base_dir: PathBuf,
    #[allow(dead_code)]
    file_name: PathBuf,
    #[allow(dead_code)]
    data: T,
}

impl<T> Config<T>
where
    T: Serialize + DeserializeOwned + Default,
{
    /// Creates a new Config instance, loading existing config if present
    /// This interface is gross -- we require the file path and the file basename? Eww.
    pub fn new(mode: ConfigMode, file_name: PathBuf) -> io::Result<Self> {
        let base_dir = match mode {
            ConfigMode::System => {
                if cfg!(target_os = "macos") {
                    PathBuf::from("/Library/Application Support/pando")
                } else {
                    PathBuf::from("/etc/pando")
                }
            }
            ConfigMode::User => {
                if cfg!(target_os = "macos") {
                    let home = env::var("HOME").map_err(|e| {
                        io::Error::new(
                            io::ErrorKind::NotFound,
                            format!("HOME environment variable not found: {}", e),
                        )
                    })?;
                    PathBuf::from(home).join("Library/Application Support/pando")
                } else {
                    // Follow XDG spec for Linux/Unix systems
                    let xdg_config_home = env::var("XDG_CONFIG_HOME").unwrap_or_else(|_| {
                        let home = env::var("HOME").expect("HOME environment variable not found");
                        format!("{}/.config", home)
                    });
                    PathBuf::from(xdg_config_home).join("pando")
                }
            }
            ConfigMode::Path(ref path) => path.parent().unwrap().to_path_buf(),
        };

        let config = Config {
            mode,
            base_dir,
            file_name,
            data: T::default(),
        };

        // Try to load existing config

        match config.load() {
            Ok(loaded_data) => Ok(Config {
                data: loaded_data,
                ..config
            }),
            Err(e) => match e.kind() {
                io::ErrorKind::NotFound => Ok(config),
                _ => Err(e),
            },
        }
    }

    /// Returns the path to the main configuration file
    pub fn config_file(&self) -> PathBuf {
        self.base_dir.join(self.file_name.clone())
    }

    /// Returns the path to the directory containing configuration files
    #[allow(dead_code)]
    pub fn config_dir(&self) -> &Path {
        &self.base_dir
    }

    /// Creates all necessary directories for the configuration
    pub fn ensure_config_dirs(&self) -> io::Result<()> {
        fs::create_dir_all(&self.base_dir)
    }

    /// Returns true if running in system mode
    #[allow(dead_code)]
    pub fn is_system(&self) -> bool {
        matches!(self.mode, ConfigMode::System)
    }

    /// Returns true if running in user mode
    #[allow(dead_code)]
    pub fn is_user(&self) -> bool {
        matches!(self.mode, ConfigMode::User)
    }

    /// Gets a reference to the configuration data
    pub fn data(&self) -> &T {
        &self.data
    }

    /// Gets a mutable reference to the configuration data
    pub fn data_mut(&mut self) -> &mut T {
        &mut self.data
    }

    /// Loads the configuration from disk
    pub fn load(&self) -> io::Result<T> {
        let mut file = fs::File::open(self.config_file())?;
        let mut contents = String::new();
        file.read_to_string(&mut contents)?;

        serde_json::from_str(&contents).map_err(|e| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("Failed to parse config file: {}", e),
            )
        })
    }

    /// Saves the configuration to disk
    pub fn save(&self) -> io::Result<()> {
        self.ensure_config_dirs()?;

        // Pretty print the JSON for easier manual editing and jq usage
        let json = serde_json::to_string_pretty(&self.data).map_err(|e| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("Failed to serialize config: {}", e),
            )
        })?;

        let mut file = fs::File::create(self.config_file())?;
        file.write_all(json.as_bytes())
    }

    /// Ensures the configuration file exists and is valid
    pub fn setup(&self) -> io::Result<T> {
        self.ensure_config_dirs()?;
        if self.config_file().exists() {
            self.load()
        } else {
            // Create a new config file with default values
            let default_data = T::default();
            self.save()?;
            Ok(default_data)
        }
    }
}

#[cfg(test)]
mod tests {

    use super::*;

    use assert_fs::prelude::*;
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Default, Serialize, Deserialize, PartialEq)]
    struct TestConfig {
        name: String,
        value: i32,
    }

    #[test]
    fn test_config_paths() {
        let config: Config<TestConfig> =
            Config::new(ConfigMode::User, "config.json".parse().unwrap()).unwrap();

        if cfg!(target_os = "macos") {
            let home = env::var("HOME").unwrap();
            assert_eq!(
                config.config_file(),
                PathBuf::from(format!(
                    "{}/Library/Application Support/pando/config.json",
                    home
                ))
            );
        } else {
            let xdg_config_home = env::var("XDG_CONFIG_HOME")
                .unwrap_or_else(|_| format!("{}/.config", env::var("HOME").unwrap()));
            assert_eq!(
                config.config_file(),
                PathBuf::from(format!("{}/pando/config.json", xdg_config_home))
            );
        }
    }

    #[test]
    fn test_save_load_config() -> io::Result<()> {
        let mut config: Config<TestConfig> =
            Config::new(ConfigMode::User, "config.json".parse().unwrap())?;

        // Modify the config
        config.data_mut().name = "test".to_string();
        config.data_mut().value = 42;

        // Save it
        config.save()?;

        // Load it in a new instance
        let loaded: Config<TestConfig> =
            Config::new(ConfigMode::User, "config.json".parse().unwrap())?;

        assert_eq!(loaded.data().name, "test");
        assert_eq!(loaded.data().value, 42);

        Ok(())
    }

    #[derive(Debug, Default, Serialize, Deserialize, PartialEq)]
    struct TestConfigJson {
        #[serde(rename = "uuid")]
        uuid: String,
    }

    #[test]
    fn test_empty_config_file() -> io::Result<()> {
        let file = assert_fs::NamedTempFile::new("config.json").map_err(|e| {
            io::Error::new(
                io::ErrorKind::Other,
                format!("Failed to create temp file: {}", e),
            )
        })?;

        let config: Config<TestConfig> = Config::new(
            ConfigMode::Path(file.path().to_path_buf()),
            file.path().to_path_buf(),
        )?;
        // Config::new(ConfigMode::User, Some(file.path().to_path_buf()

        config.save()?;

        let loaded: Config<TestConfig> = Config::new(
            ConfigMode::Path(file.path().to_path_buf()),
            file.path().to_path_buf(),
        )?;
        assert_eq!(loaded.data(), &TestConfig::default());

        Ok(())
    }

    #[test]
    fn test_freshly_configured_config_json() -> io::Result<()> {
        let file = assert_fs::NamedTempFile::new("config.json").unwrap();
        file.write_str("{\"uuid\":\"00000000-0000-0000-0000-0000DECAFBAD\"}")
            .unwrap();

        let config: Config<TestConfigJson> = Config::new(
            ConfigMode::Path(file.path().to_path_buf()),
            file.path().to_path_buf(),
        )?;
        assert_eq!(
            config.data(),
            &TestConfigJson {
                uuid: "00000000-0000-0000-0000-0000DECAFBAD".to_string()
            }
        );

        Ok(())
    }
}
