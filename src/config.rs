use anyhow::Result;
use serde::Deserialize;
use std::fs;

/// 完整的配置结构
#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    pub grpc: GrpcConfig,
    pub monitor: MonitorConfig,
    pub database: DatabaseConfig,
    pub api: ApiConfig,
}

/// gRPC 配置
#[derive(Debug, Clone, Deserialize)]
pub struct GrpcConfig {
    pub endpoint: String,
    pub timeout: u64,
    pub connect_timeout: u64,
}

/// 监控配置
#[derive(Debug, Clone, Deserialize)]
pub struct MonitorConfig {
    pub include_failed_transactions: bool,
    pub include_vote_transactions: bool,
    #[allow(dead_code)]
    pub exclude_programs: Vec<String>,
}

/// 数据库配置
#[derive(Debug, Clone, Deserialize)]
pub struct DatabaseConfig {
    pub db_path: String,
    pub key_prefix_length: usize,
    pub signature_key_prefix: String,
    pub address_key_prefix: String,
    pub max_address_records: usize,
}

/// API 服务器配置
#[derive(Debug, Clone, Deserialize)]
pub struct ApiConfig {
    pub host: String,
    pub port: u16,
    pub enable_cors: bool,
    pub log_level: String,
}

impl Config {
    /// 从配置文件加载配置
    pub fn load() -> Result<Self> {
        let config_content = fs::read_to_string("config.toml")?;
        let config: Config = toml::from_str(&config_content)?;
        Ok(config)
    }
} 