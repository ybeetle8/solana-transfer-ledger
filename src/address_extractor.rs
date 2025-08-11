use anyhow::Result;
use std::collections::HashSet;
use yellowstone_grpc_proto::prelude::SubscribeUpdateTransaction;

/// 地址提取器
pub struct AddressExtractor;

impl AddressExtractor {
    /// 从交易更新中提取所有相关地址，返回 base58 编码的地址列表
    pub fn extract_all_addresses(transaction_update: &SubscribeUpdateTransaction) -> Result<Vec<String>> {
        let mut addresses = HashSet::new();
        
        if let Some(tx_info) = &transaction_update.transaction {
            // 1. 提取主账户地址
            if let Some(transaction) = &tx_info.transaction {
                if let Some(message) = &transaction.message {
                    // 主账户地址
                    for account_key in &message.account_keys {
                        addresses.insert(bs58::encode(account_key).into_string());
                    }
                    
                    // 地址表查找中的地址
                    for lookup in &message.address_table_lookups {
                        addresses.insert(bs58::encode(&lookup.account_key).into_string());
                    }
                }
            }
            
            // 2. 提取执行元数据中的地址
            if let Some(meta) = &tx_info.meta {
                // 加载的可写地址
                for address_bytes in &meta.loaded_writable_addresses {
                    addresses.insert(bs58::encode(address_bytes).into_string());
                }
                
                // 加载的只读地址
                for address_bytes in &meta.loaded_readonly_addresses {
                    addresses.insert(bs58::encode(address_bytes).into_string());
                }
                
                // 代币相关地址
                for token_balance in &meta.pre_token_balances {
                    if !token_balance.mint.is_empty() && token_balance.mint != "11111111111111111111111111111111" {
                        addresses.insert(token_balance.mint.clone());
                    }
                    if !token_balance.owner.is_empty() && token_balance.owner != "11111111111111111111111111111111" {
                        addresses.insert(token_balance.owner.clone());
                    }
                }
                
                for token_balance in &meta.post_token_balances {
                    if !token_balance.mint.is_empty() && token_balance.mint != "11111111111111111111111111111111" {
                        addresses.insert(token_balance.mint.clone());
                    }
                    if !token_balance.owner.is_empty() && token_balance.owner != "11111111111111111111111111111111" {
                        addresses.insert(token_balance.owner.clone());
                    }
                }
                
                // 奖励接收者地址
                for reward in &meta.rewards {
                    if !reward.pubkey.is_empty() {
                        addresses.insert(reward.pubkey.clone());
                    }
                }
                
                // 返回数据程序地址
                if let Some(return_data) = &meta.return_data {
                    addresses.insert(bs58::encode(&return_data.program_id).into_string());
                }
            }
        }
        
        // 转换为 Vec 并返回
        Ok(addresses.into_iter().collect())
    }
} 