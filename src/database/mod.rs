pub mod storage;
pub mod signature_storage;
pub mod test_example;

pub use storage::{StorageManager, StorageResult, KeyValue};
pub use signature_storage::{
    SignatureStorage, 
    SignatureTransactionData, 
    SolTransfer, 
    TokenTransfer, 
    ExtractedAddresses,
    SignatureStorageStats,
};

use anyhow::Result;
use crate::config::Config;
use tracing::info;

/// 数据库管理器，统一管理所有存储组件
pub struct DatabaseManager {
    storage: StorageManager,
    signature_storage: SignatureStorage,
}

impl DatabaseManager {
    /// 从配置创建数据库管理器
    pub fn from_config(config: &Config) -> Result<Self> {
        info!("初始化数据库管理器...");
        
        // 创建存储管理器
        let storage = StorageManager::new(
            &config.database.db_path,
            config.database.key_prefix_length,
        )?;

        // 创建签名存储管理器
        let signature_storage = SignatureStorage::new(
            storage.clone(),
            config.database.signature_key_prefix.clone(),
        );

        info!("数据库管理器初始化完成");
        
        Ok(DatabaseManager {
            storage,
            signature_storage,
        })
    }

    /// 获取存储管理器引用
    pub fn storage(&self) -> &StorageManager {
        &self.storage
    }

    /// 获取签名存储管理器引用
    pub fn signature_storage(&self) -> &SignatureStorage {
        &self.signature_storage
    }

    /// 获取数据库统计信息
    pub fn get_database_stats(&self) -> Result<String> {
        self.storage.get_stats()
    }

    /// 压缩数据库
    pub fn compact_database(&self) -> Result<StorageResult> {
        self.storage.compact()
    }
} 