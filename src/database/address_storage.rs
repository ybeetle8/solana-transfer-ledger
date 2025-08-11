use anyhow::Result;
use serde::{Deserialize, Serialize};
use tracing::{debug, info};
use crate::database::storage::{StorageManager, StorageResult};
use crate::transfer_parser::{SolTransfer, TokenTransfer};

/// 地址交易记录项
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AddressTransactionRecord {
    /// 交易签名
    pub signature: String,
    /// 交易时间戳
    pub timestamp: u64,
    /// 交易槽位
    pub slot: u64,
    /// SOL转账记录
    pub sol_transfer: Option<SolTransfer>,
    /// 代币转账记录  
    pub token_transfer: Option<TokenTransfer>,
    /// 记录类型（发送还是接收）
    pub record_type: RecordType,
}

/// 记录类型
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RecordType {
    /// 发送方
    Sender,
    /// 接收方
    Receiver,
}

/// 地址交易记录列表
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AddressTransactionList {
    /// 地址
    pub address: String,
    /// 交易记录列表（索引0是最新的）
    pub records: Vec<AddressTransactionRecord>,
    /// 最后更新时间
    pub last_updated: u64,
}

/// 地址存储管理器
#[derive(Debug, Clone)]
pub struct AddressStorage {
    storage: StorageManager,
    address_prefix: String,
    max_records: usize,
}

impl AddressStorage {
    /// 创建新的地址存储实例
    pub fn new(storage: StorageManager, address_prefix: String, max_records: usize) -> Self {
        Self {
            storage,
            address_prefix,
            max_records,
        }
    }

    /// 为地址添加SOL转账记录
    pub fn add_sol_transfer(
        &self,
        address: &str,
        signature: &str,
        timestamp: u64,
        slot: u64,
        sol_transfer: SolTransfer,
        record_type: RecordType,
    ) -> Result<()> {
        let record = AddressTransactionRecord {
            signature: signature.to_string(),
            timestamp,
            slot,
            sol_transfer: Some(sol_transfer),
            token_transfer: None,
            record_type,
        };

        self.add_record(address, record)
    }

    /// 为地址添加代币转账记录
    pub fn add_token_transfer(
        &self,
        address: &str,
        signature: &str,
        timestamp: u64,
        slot: u64,
        token_transfer: TokenTransfer,
        record_type: RecordType,
    ) -> Result<()> {
        let record = AddressTransactionRecord {
            signature: signature.to_string(),
            timestamp,
            slot,
            sol_transfer: None,
            token_transfer: Some(token_transfer),
            record_type,
        };

        self.add_record(address, record)
    }

    /// 添加交易记录到地址
    fn add_record(&self, address: &str, record: AddressTransactionRecord) -> Result<()> {
        let key = format!("{}{}", self.address_prefix, address);
        
        // 获取现有记录列表
        let mut address_list = match self.storage.get::<AddressTransactionList>(&key)? {
            Some(list) => list,
            None => AddressTransactionList {
                address: address.to_string(),
                records: Vec::new(),
                last_updated: 0,
            },
        };

        // 在列表开头插入新记录（索引0是最新的）
        address_list.records.insert(0, record);
        address_list.last_updated = chrono::Utc::now().timestamp() as u64;

        // 如果记录数超过限制，删除最老的记录
        if address_list.records.len() > self.max_records {
            let removed_count = address_list.records.len() - self.max_records;
            address_list.records.truncate(self.max_records);
            debug!("地址 {} 删除了 {} 条最老的记录", address, removed_count);
        }

        // 保存更新后的列表
        self.storage.put(&key, &address_list)?;
        debug!("地址 {} 添加了新的交易记录，当前记录数: {}", address, address_list.records.len());

        Ok(())
    }

    /// 获取地址的交易记录
    pub fn get_address_records(&self, address: &str) -> Result<Option<AddressTransactionList>> {
        let key = format!("{}{}", self.address_prefix, address);
        self.storage.get(&key)
    }

    /// 获取地址的最近N条记录
    pub fn get_recent_records(&self, address: &str, limit: usize) -> Result<Vec<AddressTransactionRecord>> {
        let key = format!("{}{}", self.address_prefix, address);
        
        match self.storage.get::<AddressTransactionList>(&key)? {
            Some(list) => {
                let limit = limit.min(list.records.len());
                Ok(list.records[..limit].to_vec())
            }
            None => Ok(Vec::new()),
        }
    }

    /// 删除地址的所有记录
    pub fn delete_address_records(&self, address: &str) -> Result<StorageResult> {
        let key = format!("{}{}", self.address_prefix, address);
        self.storage.delete(&key)
    }

    /// 获取所有有记录的地址列表
    pub fn get_all_addresses(&self) -> Result<Vec<String>> {
        let keys = self.storage.get_keys_by_prefix(&self.address_prefix)?;
        let addresses: Vec<String> = keys
            .into_iter()
            .map(|key| key.strip_prefix(&self.address_prefix).unwrap_or(&key).to_string())
            .collect();
        
        debug!("找到 {} 个有交易记录的地址", addresses.len());
        Ok(addresses)
    }

    /// 获取地址统计信息
    pub fn get_address_stats(&self, address: &str) -> Result<AddressStats> {
        let records = self.get_recent_records(address, self.max_records)?;
        
        let mut sol_sent_count = 0;
        let mut sol_received_count = 0;
        let mut token_sent_count = 0;
        let mut token_received_count = 0;
        let mut total_sol_sent = 0u64;
        let mut total_sol_received = 0u64;

        for record in &records {
            match (&record.sol_transfer, &record.record_type) {
                (Some(sol), RecordType::Sender) => {
                    sol_sent_count += 1;
                    total_sol_sent += sol.amount;
                }
                (Some(sol), RecordType::Receiver) => {
                    sol_received_count += 1;
                    total_sol_received += sol.amount;
                }
                _ => {}
            }

            match (&record.token_transfer, &record.record_type) {
                (Some(_), RecordType::Sender) => token_sent_count += 1,
                (Some(_), RecordType::Receiver) => token_received_count += 1,
                _ => {}
            }
        }

        Ok(AddressStats {
            address: address.to_string(),
            total_records: records.len(),
            sol_sent_count,
            sol_received_count,
            token_sent_count,
            token_received_count,
            total_sol_sent,
            total_sol_received,
        })
    }

    /// 批量处理交易记录
    pub fn batch_process_transaction(
        &self,
        signature: &str,
        timestamp: u64,
        slot: u64,
        sol_transfers: &[SolTransfer],
        token_transfers: &[TokenTransfer],
    ) -> Result<()> {
        // 处理SOL转账
        for sol_transfer in sol_transfers {
            // 为发送方添加记录
            self.add_sol_transfer(
                &sol_transfer.from,
                signature,
                timestamp,
                slot,
                sol_transfer.clone(),
                RecordType::Sender,
            )?;

            // 为接收方添加记录
            self.add_sol_transfer(
                &sol_transfer.to,
                signature,
                timestamp,
                slot,
                sol_transfer.clone(),
                RecordType::Receiver,
            )?;
        }

        // 处理代币转账
        for token_transfer in token_transfers {
            // 为发送方添加记录
            self.add_token_transfer(
                &token_transfer.from,
                signature,
                timestamp,
                slot,
                token_transfer.clone(),
                RecordType::Sender,
            )?;

            // 为接收方添加记录
            self.add_token_transfer(
                &token_transfer.to,
                signature,
                timestamp,
                slot,
                token_transfer.clone(),
                RecordType::Receiver,
            )?;
        }

        info!("批量处理完成: 签名 {} - {} SOL转账, {} 代币转账", 
              signature, sol_transfers.len(), token_transfers.len());

        Ok(())
    }
}

/// 地址统计信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AddressStats {
    /// 地址
    pub address: String,
    /// 总记录数
    pub total_records: usize,
    /// SOL发送次数
    pub sol_sent_count: usize,
    /// SOL接收次数
    pub sol_received_count: usize,
    /// 代币发送次数
    pub token_sent_count: usize,
    /// 代币接收次数
    pub token_received_count: usize,
    /// 总SOL发送数量（lamports）
    pub total_sol_sent: u64,
    /// 总SOL接收数量（lamports）
    pub total_sol_received: u64,
} 