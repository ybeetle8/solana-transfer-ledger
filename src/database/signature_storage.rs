use anyhow::Result;
use serde::{Serialize, Deserialize};
use crate::database::storage::{StorageManager, StorageResult, KeyValue};

use tracing::{info, debug};

/// 签名交易数据结构
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignatureTransactionData {
    /// 交易签名 (base58 编码)
    pub signature: String,
    /// SOL 转账数据
    pub sol_transfers: Vec<SolTransfer>,
    /// 代币转账数据
    pub token_transfers: Vec<TokenTransfer>,
    /// 提取到的地址信息
    pub extracted_addresses: ExtractedAddresses,
    /// 交易时间戳
    pub timestamp: i64,
    /// 区块高度
    pub slot: u64,
    /// 交易是否成功
    pub is_successful: bool,
}

/// SOL 转账信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SolTransfer {
    /// 发送方地址
    pub from: String,
    /// 接收方地址
    pub to: String,
    /// 转账金额 (lamports)
    pub amount: u64,
    /// 转账类型（如：系统转账、质押等）
    pub transfer_type: String,
}

/// 代币转账信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenTransfer {
    /// 发送方地址
    pub from: String,
    /// 接收方地址
    pub to: String,
    /// 转账金额
    pub amount: u64,
    /// 代币精度
    pub decimals: u8,
    /// 代币mint地址
    pub mint: String,
    /// 代币程序ID
    pub program_id: String,
    /// 转账类型
    pub transfer_type: String,
}

/// 提取到的地址信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtractedAddresses {
    /// 所有地址
    pub all_addresses: Vec<String>,
    /// 签名者地址
    pub signers: Vec<String>,
    /// 可写地址
    pub writable_addresses: Vec<String>,
    /// 只读地址
    pub readonly_addresses: Vec<String>,
    /// 程序地址
    pub program_addresses: Vec<String>,
}

/// 签名存储管理器
pub struct SignatureStorage {
    storage: StorageManager,
    signature_prefix: String,
}

impl SignatureStorage {
    /// 创建新的签名存储管理器
    pub fn new(storage: StorageManager, signature_prefix: String) -> Self {
        Self {
            storage,
            signature_prefix,
        }
    }

    /// 存储签名交易数据
    pub fn store_signature_data(
        &self, 
        signature: &str, 
        data: &SignatureTransactionData
    ) -> Result<StorageResult> {
        let key = self.storage.make_key(&self.signature_prefix, signature)?;
        
        debug!("存储签名数据: signature={}, key={}", signature, key);
        
        self.storage.put(&key, data)
    }

    /// 根据签名获取交易数据
    pub fn get_signature_data(&self, signature: &str) -> Result<Option<SignatureTransactionData>> {
        let key = self.storage.make_key(&self.signature_prefix, signature)?;
        
        debug!("查询签名数据: signature={}, key={}", signature, key);
        
        self.storage.get(&key)
    }

    /// 检查签名是否已存在
    pub fn signature_exists(&self, signature: &str) -> Result<bool> {
        let key = self.storage.make_key(&self.signature_prefix, signature)?;
        self.storage.exists(&key)
    }

    /// 删除签名数据
    pub fn delete_signature_data(&self, signature: &str) -> Result<StorageResult> {
        let key = self.storage.make_key(&self.signature_prefix, signature)?;
        
        debug!("删除签名数据: signature={}, key={}", signature, key);
        
        self.storage.delete(&key)
    }

    /// 获取所有签名数据
    pub fn get_all_signature_data(&self) -> Result<Vec<KeyValue<SignatureTransactionData>>> {
        debug!("获取所有签名数据: prefix={}", self.signature_prefix);
        
        self.storage.get_by_prefix(&self.signature_prefix)
    }

    /// 获取所有签名键
    pub fn get_all_signature_keys(&self) -> Result<Vec<String>> {
        let keys = self.storage.get_keys_by_prefix(&self.signature_prefix)?;
        
        // 移除前缀，只返回实际的签名值
        let signatures: Vec<String> = keys
            .into_iter()
            .filter_map(|key| {
                if key.len() > self.signature_prefix.len() {
                    Some(key[self.signature_prefix.len()..].to_string())
                } else {
                    None
                }
            })
            .collect();
        
        debug!("查询到 {} 个签名", signatures.len());
        Ok(signatures)
    }

    /// 批量存储签名数据
    pub fn batch_store_signatures(
        &self, 
        signatures_data: Vec<(String, SignatureTransactionData)>
    ) -> Result<StorageResult> {
        let mut items = Vec::new();
        
        for (signature, data) in signatures_data {
            let key = self.storage.make_key(&self.signature_prefix, &signature)?;
            items.push((key, data));
        }
        
        info!("批量存储 {} 个签名数据", items.len());
        
        self.storage.batch_put(items)
    }

    /// 根据地址查找相关的签名（这需要遍历所有数据，效率较低）
    pub fn find_signatures_by_address(&self, address: &str) -> Result<Vec<String>> {
        let all_data = self.get_all_signature_data()?;
        let mut matching_signatures = Vec::new();

        for item in all_data {
            let data = item.value;
            
            // 检查是否在提取的地址中
            if data.extracted_addresses.all_addresses.contains(&address.to_string()) {
                matching_signatures.push(data.signature);
                continue;
            }
            
            // 检查SOL转账
            for transfer in &data.sol_transfers {
                if transfer.from == address || transfer.to == address {
                    matching_signatures.push(data.signature.clone());
                    break;
                }
            }
            
            // 检查代币转账（如果还没有找到匹配）
            if !matching_signatures.contains(&data.signature) {
                for transfer in &data.token_transfers {
                    if transfer.from == address || transfer.to == address {
                        matching_signatures.push(data.signature.clone());
                        break;
                    }
                }
            }
        }

        debug!("地址 {} 关联的签名数量: {}", address, matching_signatures.len());
        Ok(matching_signatures)
    }

    /// 根据时间范围查找签名
    pub fn find_signatures_by_time_range(
        &self, 
        start_timestamp: i64, 
        end_timestamp: i64
    ) -> Result<Vec<String>> {
        let all_data = self.get_all_signature_data()?;
        let mut matching_signatures = Vec::new();

        for item in all_data {
            let data = item.value;
            if data.timestamp >= start_timestamp && data.timestamp <= end_timestamp {
                matching_signatures.push(data.signature);
            }
        }

        debug!(
            "时间范围 {}-{} 内的签名数量: {}", 
            start_timestamp, 
            end_timestamp, 
            matching_signatures.len()
        );
        
        Ok(matching_signatures)
    }

    /// 获取存储统计信息
    pub fn get_statistics(&self) -> Result<SignatureStorageStats> {
        let all_keys = self.get_all_signature_keys()?;
        let total_signatures = all_keys.len();
        
        let all_data = self.get_all_signature_data()?;
        let mut total_sol_transfers = 0;
        let mut total_token_transfers = 0;
        let mut successful_transactions = 0;

        for item in all_data {
            let data = item.value;
            total_sol_transfers += data.sol_transfers.len();
            total_token_transfers += data.token_transfers.len();
            if data.is_successful {
                successful_transactions += 1;
            }
        }

        Ok(SignatureStorageStats {
            total_signatures,
            total_sol_transfers,
            total_token_transfers,
            successful_transactions,
            failed_transactions: total_signatures - successful_transactions,
        })
    }
}

/// 签名存储统计信息
#[derive(Debug, Serialize, Deserialize)]
pub struct SignatureStorageStats {
    pub total_signatures: usize,
    pub total_sol_transfers: usize,
    pub total_token_transfers: usize,
    pub successful_transactions: usize,
    pub failed_transactions: usize,
}

/// 构建签名交易数据的辅助函数
impl SignatureTransactionData {
    /// 创建新的签名交易数据
    pub fn new(
        signature: String,
        timestamp: i64,
        slot: u64,
        is_successful: bool,
    ) -> Self {
        Self {
            signature,
            sol_transfers: Vec::new(),
            token_transfers: Vec::new(),
            extracted_addresses: ExtractedAddresses {
                all_addresses: Vec::new(),
                signers: Vec::new(),
                writable_addresses: Vec::new(),
                readonly_addresses: Vec::new(),
                program_addresses: Vec::new(),
            },
            timestamp,
            slot,
            is_successful,
        }
    }

    /// 添加SOL转账
    pub fn add_sol_transfer(&mut self, transfer: SolTransfer) {
        self.sol_transfers.push(transfer);
    }

    /// 添加代币转账
    pub fn add_token_transfer(&mut self, transfer: TokenTransfer) {
        self.token_transfers.push(transfer);
    }

    /// 设置提取的地址信息
    pub fn set_extracted_addresses(&mut self, addresses: ExtractedAddresses) {
        self.extracted_addresses = addresses;
    }
} 