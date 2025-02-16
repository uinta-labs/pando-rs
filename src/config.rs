use serde::{de::DeserializeOwned, Serialize};
use std::path::{Path, PathBuf};
use std::env;
use std::fs;
use std::io::{self, Read, Write};

#[derive(Debug, Clone, PartialEq)]
pub enum ConfigMode {
    System,
    User,
}

#[derive(Debug)]
pub struct Config<T> {
    mode: ConfigMode,
    base_dir: PathBuf,
    data: T,
}

impl<T> Config<T>
where
    T: Serialize + DeserializeOwned + Default,
{
    /// Creates a new Config instance, loading existing config if present
    pub fn new(mode: ConfigMode) -> io::Result<Self> {
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
                    PathBuf::from(home)
                        .join("Library/Application Support/pando")
                } else {
                    // Follow XDG spec for Linux/Unix systems
                    let xdg_config_home = env::var("XDG_CONFIG_HOME").unwrap_or_else(|_| {
                        let home = env::var("HOME").expect("HOME environment variable not found");
                        format!("{}/.config", home)
                    });
                    PathBuf::from(xdg_config_home).join("pando")
                }
            }
        };

        let config = Config {
            mode,
            base_dir,
            data: T::default(),
        };

        // Try to load existing config
        if let Ok(loaded_data) = config.load() {
            Ok(Config {
                data: loaded_data,
                ..config
            })
        } else {
            Ok(config)
        }
    }

    /// Returns the path to the main configuration file
    pub fn config_file(&self) -> PathBuf {
        self.base_dir.join("config.json")
    }

    /// Returns the path to the directory containing configuration files
    pub fn config_dir(&self) -> &Path {
        &self.base_dir
    }

    /// Creates all necessary directories for the configuration
    pub fn ensure_config_dirs(&self) -> io::Result<()> {
        fs::create_dir_all(&self.base_dir)
    }

    /// Returns true if running in system mode
    pub fn is_system(&self) -> bool {
        matches!(self.mode, ConfigMode::System)
    }

    /// Returns true if running in user mode
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Default, Serialize, Deserialize, PartialEq)]
    struct TestConfig {
        name: String,
        value: i32,
    }

    #[test]
    fn test_config_paths() {
        let config: Config<TestConfig> = Config::new(ConfigMode::User).unwrap();

        if cfg!(target_os = "macos") {
            let home = env::var("HOME").unwrap();
            assert_eq!(
                config.config_file(),
                PathBuf::from(format!("{}/Library/Application Support/pando/config.json", home))
            );
        } else {
            let xdg_config_home = env::var("XDG_CONFIG_HOME").unwrap_or_else(|_| {
                format!("{}/.config", env::var("HOME").unwrap())
            });
            assert_eq!(
                config.config_file(),
                PathBuf::from(format!("{}/pando/config.json", xdg_config_home))
            );
        }
    }

    #[test]
    fn test_save_load_config() -> io::Result<()> {
        let mut config: Config<TestConfig> = Config::new(ConfigMode::User)?;

        // Modify the config
        config.data_mut().name = "test".to_string();
        config.data_mut().value = 42;

        // Save it
        config.save()?;

        // Load it in a new instance
        let loaded: Config<TestConfig> = Config::new(ConfigMode::User)?;

        assert_eq!(loaded.data().name, "test");
        assert_eq!(loaded.data().value, 42);

        Ok(())
    }
}