pub mod storage;
pub mod signature_storage;
pub mod address_storage;

use anyhow::Result;
pub use storage::{StorageManager, StorageResult};
pub use signature_storage::{
    SignatureStorage, SignatureTransactionData, SolTransfer, TokenTransfer,
    ExtractedAddresses,
};
pub use address_storage::{
    AddressStorage, AddressTransactionRecord, AddressTransactionList, 
    RecordType, AddressStats,
};

/// 数据库管理器
#[derive(Debug, Clone)]
pub struct DatabaseManager {
    #[allow(dead_code)]
    storage: StorageManager,
    signature_storage: SignatureStorage,
    address_storage: AddressStorage,
}

impl DatabaseManager {
    /// 创建新的数据库管理器
    pub fn new(
        db_path: &str,
        signature_prefix: String,
        address_prefix: String,
        max_address_records: usize,
    ) -> Result<Self> {
        let storage = StorageManager::new(db_path, key_prefix_length)?;
        let signature_storage = SignatureStorage::new(storage.clone(), signature_prefix);
        let address_storage = AddressStorage::new(storage.clone(), address_prefix, max_address_records);

        Ok(Self {
            storage: storage.clone(),
            signature_storage,
            address_storage,
        })
    }

    /// 获取签名存储实例
    pub fn signature_storage(&self) -> &SignatureStorage {
        &self.signature_storage
    }

    /// 获取地址存储实例
    pub fn address_storage(&self) -> &AddressStorage {
        &self.address_storage
    }

    /// 获取底层存储实例
    #[allow(dead_code)]
    pub fn storage(&self) -> &StorageManager {
        &self.storage
    }

    /// 获取数据库统计信息
    #[allow(dead_code)]
    pub fn get_database_stats(&self) -> Result<String> {
        self.storage.get_stats()
    }

    /// 压缩数据库
    #[allow(dead_code)]
    pub fn compact_database(&self) -> Result<StorageResult> {
        self.storage.compact()
    }
} 