use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub app: AppConfig,
    pub database: DatabaseConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub name: String,
    pub version: String,
    pub host: String,
    pub port: u16,
    pub log_level: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatabaseConfig {
    pub host: String,
    pub port: u16,
    pub username: String,
    pub password: String,
    pub database: String,
    pub pool_size: u32,
    pub timeout_seconds: u64,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            app: AppConfig {
                name: "axum-health-service".to_string(),
                version: "0.1.0".to_string(),
                host: "0.0.0.0".to_string(),
                port: 3000,
                log_level: "info".to_string(),
            },
            database: DatabaseConfig {
                host: "localhost".to_string(),
                port: 3306,
                username: "user".to_string(),
                password: "password".to_string(),
                database: "health_service".to_string(),
                pool_size: 10,
                timeout_seconds: 30,
            },
        }
    }
}

impl Config {
    /// Load configuration from a TOML file
    pub fn from_file(path: &str) -> Result<Self> {
        let content = fs::read_to_string(path)
            .with_context(|| format!("Failed to read config file: {}", path))?;
        
        let config: Config = toml::from_str(&content)
            .with_context(|| format!("Failed to parse TOML config file: {}", path))?;
        
        Ok(config)
    }
    
    /// Save configuration to a TOML file
    pub fn to_file(&self, path: &str) -> Result<()> {
        let content = toml::to_string_pretty(self)
            .context("Failed to serialize config to TOML")?;
        
        fs::write(path, content)
            .with_context(|| format!("Failed to write config file: {}", path))?;
        
        Ok(())
    }
    
    /// Load configuration from file, or create default if file doesn't exist
    pub fn load_or_default(path: &str) -> Result<Self> {
        match Self::from_file(path) {
            Ok(config) => Ok(config),
            Err(_) => {
                let default_config = Self::default();
                default_config.to_file(path)
                    .with_context(|| format!("Failed to create default config file: {}", path))?;
                Ok(default_config)
            }
        }
    }
    
    /// Get database connection string
    pub fn database_url(&self) -> String {
        format!(
            "mysql://{}:{}@{}:{}/{}",
            self.database.username,
            self.database.password,
            self.database.host,
            self.database.port,
            self.database.database
        )
    }
    
    /// Get server bind address
    pub fn bind_address(&self) -> String {
        format!("{}:{}", self.app.host, self.app.port)
    }
}
