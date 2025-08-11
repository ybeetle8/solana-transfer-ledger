use anyhow::Result;
use solana_transfer_ledger::{
    config::Config,
    database::{DatabaseManager, RecordType},
    transfer_parser::{SolTransfer, TokenTransfer},
};
use tracing::{info, error};
use chrono::Utc;

#[tokio::main]
async fn main() -> Result<()> {
    // 初始化日志
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    info!("🧪 开始地址存储功能测试");

    // 加载配置
    let config = Config::load()?;
    
    // 创建数据库管理器
    let db_manager = DatabaseManager::new(
        &config.database.db_path,
        config.database.signature_key_prefix.clone(),
        config.database.address_key_prefix.clone(),
        config.database.max_address_records,
    )?;

    info!("✅ 数据库管理器初始化成功");

    // 示例1: 添加SOL转账记录
    let test_address1 = "7EqQdEULxWcraVx3tXzSFz1hbCqkrvBdBdXkxjt7FuSY";
    let test_address2 = "DfXygSm4jCyNCybVYYK6DwvWqjKee8pbDmJGcLWNDXjh";
    let test_signature = "5j7s88vNfuTXpDR8J9X8jF7VqL4vGHfJw9KYg4A9F1CvwYCQj2DjLhQ8X9zL7pYnR2vZ5X3s8KcW6t9A2FhQ1vB";
    let timestamp = Utc::now().timestamp() as u64;
    let slot = 123456789;

    let sol_transfer = SolTransfer {
        signature: test_signature.to_string(),
        from: test_address1.to_string(),
        to: test_address2.to_string(),
        from_index: 0,
        to_index: 1,
        amount: 1_000_000_000, // 1 SOL
        transfer_type: "SOL Transfer".to_string(),
    };

    // 为发送方添加记录
    db_manager.address_storage().add_sol_transfer(
        test_address1,
        test_signature,
        timestamp,
        slot,
        sol_transfer.clone(),
        RecordType::Sender,
    )?;
    info!("✅ 为发送方地址 {} 添加SOL转账记录", test_address1);

    // 为接收方添加记录
    db_manager.address_storage().add_sol_transfer(
        test_address2,
        test_signature,
        timestamp,
        slot,
        sol_transfer,
        RecordType::Receiver,
    )?;
    info!("✅ 为接收方地址 {} 添加SOL转账记录", test_address2);

    // 示例2: 添加代币转账记录
    let token_signature = "3k8h9vNfuTXpDR8J9X8jF7VqL4vGHfJw9KYg4A9F1CvwYCQj2DjLhQ8X9zL7pYnR2vZ5X3s8KcW6t9A2FhQ1vB";
    let token_transfer = TokenTransfer {
        signature: token_signature.to_string(),
        from: test_address1.to_string(),
        to: test_address2.to_string(),
        amount: 1000000, // 1 USDC (6 decimals)
        decimals: 6,
        mint: "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v".to_string(), // USDC mint
        program_id: "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA".to_string(),
        transfer_type: "Token Transfer".to_string(),
    };

    // 为发送方添加代币转账记录
    db_manager.address_storage().add_token_transfer(
        test_address1,
        token_signature,
        timestamp + 10,
        slot + 1,
        token_transfer.clone(),
        RecordType::Sender,
    )?;
    info!("✅ 为发送方地址 {} 添加代币转账记录", test_address1);

    // 为接收方添加代币转账记录
    db_manager.address_storage().add_token_transfer(
        test_address2,
        token_signature,
        timestamp + 10,
        slot + 1,
        token_transfer,
        RecordType::Receiver,
    )?;
    info!("✅ 为接收方地址 {} 添加代币转账记录", test_address2);

    // 示例3: 查询地址交易记录
    match db_manager.address_storage().get_address_records(test_address1)? {
        Some(records) => {
            info!("✅ 地址 {} 的交易记录:", test_address1);
            info!("  总记录数: {}", records.records.len());
            info!("  最后更新: {}", records.last_updated);
            for (i, record) in records.records.iter().enumerate() {
                info!("  记录 {}: 签名 {}, 类型 {:?}", 
                      i + 1, &record.signature[..8], record.record_type);
            }
        }
        None => {
            info!("地址 {} 没有交易记录", test_address1);
        }
    }

    // 示例4: 获取地址统计信息
    let stats = db_manager.address_storage().get_address_stats(test_address1)?;
    info!("✅ 地址 {} 统计信息:", test_address1);
    info!("  总记录数: {}", stats.total_records);
    info!("  SOL发送次数: {}", stats.sol_sent_count);
    info!("  SOL接收次数: {}", stats.sol_received_count);
    info!("  代币发送次数: {}", stats.token_sent_count);
    info!("  代币接收次数: {}", stats.token_received_count);
    info!("  总SOL发送: {} lamports ({:.6} SOL)", 
          stats.total_sol_sent, stats.total_sol_sent as f64 / 1_000_000_000.0);
    info!("  总SOL接收: {} lamports ({:.6} SOL)", 
          stats.total_sol_received, stats.total_sol_received as f64 / 1_000_000_000.0);

    // 示例5: 获取所有有记录的地址
    let addresses = db_manager.address_storage().get_all_addresses()?;
    info!("✅ 数据库中有记录的地址数量: {}", addresses.len());
    for (i, address) in addresses.iter().take(10).enumerate() {
        info!("  地址 {}: {}", i + 1, address);
    }

    // 示例6: 批量处理交易测试
    let batch_signature = "9m7n8vNfuTXpDR8J9X8jF7VqL4vGHfJw9KYg4A9F1CvwYCQj2DjLhQ8X9zL7pYnR2vZ5X3s8KcW6t9A2FhQ1vB";
    let sol_transfers = vec![
        SolTransfer {
            signature: batch_signature.to_string(),
            from: test_address1.to_string(),
            to: test_address2.to_string(),
            from_index: 0,
            to_index: 1,
            amount: 500_000_000, // 0.5 SOL
            transfer_type: "SOL Transfer".to_string(),
        }
    ];
    let token_transfers = vec![
        TokenTransfer {
            signature: batch_signature.to_string(),
            from: test_address2.to_string(),
            to: test_address1.to_string(),
            amount: 2000000, // 2 USDC
            decimals: 6,
            mint: "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v".to_string(),
            program_id: "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA".to_string(),
            transfer_type: "Token Transfer".to_string(),
        }
    ];

    db_manager.address_storage().batch_process_transaction(
        batch_signature,
        timestamp + 20,
        slot + 2,
        &sol_transfers,
        &token_transfers,
    )?;
    info!("✅ 批量处理交易完成");

    // 再次检查地址统计信息
    let updated_stats = db_manager.address_storage().get_address_stats(test_address1)?;
    info!("✅ 更新后地址 {} 统计信息:", test_address1);
    info!("  总记录数: {}", updated_stats.total_records);
    info!("  SOL发送次数: {}", updated_stats.sol_sent_count);
    info!("  SOL接收次数: {}", updated_stats.sol_received_count);
    info!("  代币发送次数: {}", updated_stats.token_sent_count);
    info!("  代币接收次数: {}", updated_stats.token_received_count);

    info!("🎉 地址存储功能测试完成！");
    Ok(())
} 