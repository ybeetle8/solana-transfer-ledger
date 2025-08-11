use anyhow::Result;
use futures::stream::StreamExt;
use std::{collections::HashMap, time::Duration};
use tonic::transport::ClientTlsConfig;
use tracing::{error, info, warn};
use yellowstone_grpc_client::GeyserGrpcClient;
use yellowstone_grpc_proto::prelude::{
    subscribe_update::UpdateOneof, CommitmentLevel, SubscribeRequest,
    SubscribeRequestFilterTransactions, SubscribeUpdate,
};

use crate::config::{GrpcConfig, MonitorConfig};
use crate::transfer_parser::TransferParser;

/// Solana gRPC 客户端
pub struct SolanaGrpcClient {
    grpc_config: GrpcConfig,
    monitor_config: MonitorConfig,
}

impl SolanaGrpcClient {
    /// 创建新的 gRPC 客户端
    pub fn new(grpc_config: GrpcConfig, monitor_config: MonitorConfig) -> Self {
        Self {
            grpc_config,
            monitor_config,
        }
    }

    /// 开始监听并打印 gRPC 数据
    pub async fn start_monitoring(&self) -> Result<()> {
        info!("🚀 开始启动 Solana gRPC 客户端");
        info!("📝 配置信息:");
        info!("  - gRPC 端点: {}", self.grpc_config.endpoint);
        info!("  - 连接超时: {}秒", self.grpc_config.connect_timeout);
        info!("  - 请求超时: {}秒", self.grpc_config.timeout);
        info!("  - 包含失败交易: {}", self.monitor_config.include_failed_transactions);
        info!("  - 包含投票交易: {}", self.monitor_config.include_vote_transactions);


        loop {
            match self.connect_and_subscribe().await {
                Ok(_) => {
                    info!("🔄 连接断开，准备重连...");
                }
                Err(e) => {
                    error!("❌ 连接失败: {}", e);
                    info!("⏰ 5秒后重试...");
                    tokio::time::sleep(Duration::from_secs(5)).await;
                }
            }
        }
    }

    /// 尝试连接并订阅数据
    async fn connect_and_subscribe(&self) -> Result<()> {
        info!("🔗 正在连接到 gRPC 端点: {}", self.grpc_config.endpoint);

        // 配置 TLS
        let tls_config = ClientTlsConfig::new().with_native_roots();

        // 创建订阅请求 - 修改为更简单的配置来获取更多数据
        let subscribe_request = SubscribeRequest {
            accounts: HashMap::new(),
            slots: HashMap::from([(
                "slot".to_string(),
                yellowstone_grpc_proto::prelude::SubscribeRequestFilterSlots {
                    filter_by_commitment: Some(true),
                    interslot_updates: Some(false),
                },
            )]),
            transactions: HashMap::from([(
                "txn".to_string(),
                SubscribeRequestFilterTransactions {
                    vote: Some(false), // 不包含投票交易以减少噪音
                    failed: Some(false), // 不包含失败交易
                    signature: None,
                    account_include: vec![], // 移除特定账户限制以获取更多交易
                    account_exclude: vec![],
                    account_required: vec![],
                },
            )]),
            transactions_status: HashMap::new(),
            blocks: HashMap::new(),
            blocks_meta: HashMap::new(),
            entry: HashMap::new(),
            accounts_data_slice: vec![],
            commitment: Some(CommitmentLevel::Processed as i32),
            from_slot: None,
            ping: None,
        };

        info!("✅ 成功连接到 gRPC 服务器，开始订阅数据...");

        // 建立连接并订阅
        let mut stream = GeyserGrpcClient::build_from_shared(self.grpc_config.endpoint.clone())?
            .tls_config(tls_config)?
            .timeout(Duration::from_secs(self.grpc_config.timeout))
            .connect_timeout(Duration::from_secs(self.grpc_config.connect_timeout))
            .connect()
            .await?
            .subscribe_once(subscribe_request)
            .await?;

        info!("📡 开始监听 Solana 数据流...");
        let mut message_count = 0u64;
        let mut transaction_count = 0u64;

        while let Some(message) = stream.next().await {
            match message {
                Ok(update) => {
                    message_count += 1;
                    self.handle_update(update, &mut transaction_count, &mut message_count)
                        .await?;
                }
                Err(e) => {
                    error!("❌ 接收消息时出错: {:?}", e);
                    return Err(e.into());
                }
            }
        }

        Ok(())
    }

    /// 处理接收到的更新消息
    async fn handle_update(
        &self,
        update: SubscribeUpdate,
        transaction_count: &mut u64,
        message_count: &mut u64,
    ) -> Result<()> {
        // 每1000条消息打印一次统计
        if *message_count % 1000 == 0 {
            info!("📊 已处理 {} 条消息，其中 {} 条交易", message_count, transaction_count);
        }

        match update.update_oneof {
            Some(UpdateOneof::Transaction(transaction_update)) => {
                *transaction_count += 1;
                self.print_transaction_info(&transaction_update, *transaction_count);
                
                // 解析SOL转账
                self.parse_and_print_transfers(&transaction_update);
            }
            Some(UpdateOneof::Account(account_update)) => {
                self.print_account_info(&account_update);
            }
            Some(UpdateOneof::Slot(slot_update)) => {
                self.print_slot_info(&slot_update);
            }
            Some(UpdateOneof::Block(block_update)) => {
                self.print_block_info(&block_update);
            }
            Some(UpdateOneof::Ping(_)) => {
                // info!("🏓 收到 Ping 消息");
            }
            Some(UpdateOneof::Pong(_)) => {
                // info!("🏓 收到 Pong 消息");
            }
            Some(UpdateOneof::BlockMeta(block_meta)) => {
                self.print_block_meta_info(&block_meta);
            }
            Some(UpdateOneof::Entry(entry_update)) => {
                self.print_entry_info(&entry_update);
            }
            Some(UpdateOneof::TransactionStatus(tx_status)) => {
                self.print_transaction_status_info(&tx_status);
            }
            None => {
                warn!("⚠️ 收到空消息");
            }
        }

        Ok(())
    }

    /// 打印交易信息
    fn print_transaction_info(
        &self,
        transaction_update: &yellowstone_grpc_proto::prelude::SubscribeUpdateTransaction,
        _count: u64,
    ) {
        // info!("💰 交易 #{} - 槽位: {}", count, transaction_update.slot);
        
        if let Some(tx) = &transaction_update.transaction {
            let signature = bs58::encode(&tx.signature).into_string();
            info!("📝 签名: {}", &signature);
            
            // if let Some(meta) = &tx.meta {
            //     if let Some(err) = &meta.err {
            //         info!("   ❌ 错误: {:?}", err);
            //     } else {
            //         info!("   ✅ 执行成功");
            //     }
                
            //     if let Some(compute_units) = meta.compute_units_consumed {
            //         info!("   🔧 计算单元消耗: {}", compute_units);
            //     }
                
            //     info!("   💸 手续费: {} lamports", meta.fee);
                
            //     // 打印所有账户余额信息
            //     if !meta.pre_balances.is_empty() {
            //         info!("   💰 账户余额信息:");
            //         info!("     执行前余额 (pre_balances): {:?}", meta.pre_balances);
            //         if !meta.post_balances.is_empty() {
            //             info!("     执行后余额 (post_balances): {:?}", meta.post_balances);
            //             info!("     余额变化:");
            //             for (i, (pre, post)) in meta.pre_balances.iter().zip(meta.post_balances.iter()).enumerate() {
            //                 if pre != post {
            //                     let change = *post as i64 - *pre as i64;
            //                     let sol_change = change as f64 / 1_000_000_000.0;
            //                     info!("       账户 {}: {} -> {} lamports (变化: {} lamports / {:.9} SOL)", 
            //                           i, pre, post, change, sol_change);
            //                 }
            //             }
            //         }
            //     }
            // }
        }
        // println!(); // 空行分隔
    }

    /// 打印账户信息
    fn print_account_info(
        &self,
        account_update: &yellowstone_grpc_proto::prelude::SubscribeUpdateAccount,
    ) {
        if let Some(account) = &account_update.account {
            let pubkey = bs58::encode(&account.pubkey).into_string();
            info!("👤 账户更新 - 地址: {}", pubkey);
            info!("   📍 槽位: {}", account_update.slot);
            info!("   💰 余额: {} lamports", account.lamports);
            info!("   👑 所有者: {}", bs58::encode(&account.owner).into_string());
            info!("   📊 数据长度: {} bytes", account.data.len());
        }
    }

    /// 打印槽位信息
    fn print_slot_info(&self, slot_update: &yellowstone_grpc_proto::prelude::SubscribeUpdateSlot) {
        info!("🎯 槽位更新 - 槽位: {}", slot_update.slot);
        info!("   📈 状态: {:?}", slot_update.status());
        if let Some(parent) = slot_update.parent {
            info!("   👆 父槽位: {}", parent);
        }
    }

    /// 打印区块信息
    fn print_block_info(&self, block_update: &yellowstone_grpc_proto::prelude::SubscribeUpdateBlock) {
        info!("🧱 区块更新 - 槽位: {}", block_update.slot);
        info!("   🔗 区块哈希: {}", bs58::encode(&block_update.blockhash).into_string());
        info!("   📊 交易数量: {}", block_update.transactions.len());
        info!("   ⏰ 区块时间: {:?}", block_update.block_time);
    }

    /// 打印区块元数据信息
    fn print_block_meta_info(
        &self,
        block_meta: &yellowstone_grpc_proto::prelude::SubscribeUpdateBlockMeta,
    ) {
        info!("📋 区块元数据 - 槽位: {}", block_meta.slot);
        info!("   🔗 区块哈希: {}", bs58::encode(&block_meta.blockhash).into_string());
        info!("   ⏰ 区块时间: {:?}", block_meta.block_time);
    }

    /// 打印条目信息
    fn print_entry_info(&self, entry_update: &yellowstone_grpc_proto::prelude::SubscribeUpdateEntry) {
        info!("📝 条目更新 - 槽位: {}", entry_update.slot);
        info!("   🆔 索引: {}", entry_update.index);
        info!("   📊 交易数量: {}", entry_update.num_hashes);
        info!("   🔗 哈希: {}", bs58::encode(&entry_update.hash).into_string());
    }

    /// 打印交易状态信息
    fn print_transaction_status_info(
        &self,
        tx_status: &yellowstone_grpc_proto::prelude::SubscribeUpdateTransactionStatus,
    ) {
        let signature = bs58::encode(&tx_status.signature).into_string();
        info!("🔍 交易状态 - 签名: {}", &signature[..32]);
        info!("   📍 槽位: {}", tx_status.slot);
        info!("   📋 索引: {}", tx_status.index);
        if let Some(err) = &tx_status.err {
            info!("   ❌ 错误: {:?}", err);
        } else {
            info!("   ✅ 执行成功");
        }
    }

    /// 解析并打印SOL转账信息
    fn parse_and_print_transfers(&self, transaction_update: &yellowstone_grpc_proto::prelude::SubscribeUpdateTransaction) {
        // 解析SOL转账
        match TransferParser::parse_sol_transfers(transaction_update) {
            Ok(transfers) => {
                if !transfers.is_empty() {
                    TransferParser::print_transfers(&transfers);
                    
                    // // 统计信息
                    // let total_amount = TransferParser::get_total_transfer_amount(&transfers);
                    // if total_amount > 0 {
                    //     let total_sol = total_amount as f64 / 1_000_000_000.0;
                    //     info!("   📊 SOL总转账金额: {} lamports ({:.9} SOL)", total_amount, total_sol);
                    // }
                    
                    // // 标记大额转账
                    // if TransferParser::has_large_transfer(&transfers, 10.0) {
                    //     info!("   🚨 检测到大额SOL转账（>10 SOL）！");
                    // }
                }
            }
            Err(e) => {
                warn!("解析SOL转账时出错: {}", e);
            }
        }

        // 解析代币转账
        match TransferParser::parse_token_transfers(transaction_update) {
            Ok(token_transfers) => {
                if !token_transfers.is_empty() {
                    TransferParser::print_token_transfers(&token_transfers);
                    
                    // // 统计信息
                    // let token_count = TransferParser::get_total_token_transfer_count(&token_transfers);
                    // info!("   📊 代币转账总数: {} 笔", token_count);
                    
                    // // 按代币分组统计
                    // let grouped = TransferParser::group_token_transfers_by_mint(&token_transfers);
                    // if grouped.len() > 1 {
                    //     info!("   🏷️  涉及 {} 种不同代币", grouped.len());
                    //     for (mint, mint_transfers) in &grouped {
                    //         info!("     代币 {}: {} 笔转账", &mint[..8], mint_transfers.len());
                    //     }
                    // }
                }
            }
            Err(e) => {
                warn!("解析代币转账时出错: {}", e);
            }
        }
    }
} 