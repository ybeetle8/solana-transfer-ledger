use anyhow::Result;
use tracing::{error, info, debug};
use tracing_subscriber;

use solana_transfer_ledger::{
    Config,
    DatabaseManager, 
    SignatureTransactionData, 
    SolTransfer, 
    TokenTransfer, 
    ExtractedAddresses,
    StorageManager,
};

#[tokio::main]
async fn main() -> Result<()> {
    // 初始化日志
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .init();

    info!("🧪 开始测试 RocksDB 数据库功能...");

    // 测试键前缀功能
    if let Err(e) = demonstrate_key_prefix() {
        error!("❌ 键前缀演示失败: {}", e);
        return Err(e);
    }

    // 运行数据库示例
    if let Err(e) = run_database_example().await {
        error!("❌ 数据库示例运行失败: {}", e);
        return Err(e);
    }

    info!("✅ 所有数据库测试已完成！");
    Ok(())
}

/// 数据库使用示例
async fn run_database_example() -> Result<()> {
    info!("🔧 运行数据库示例...");

    // 加载配置
    let config = Config::load()?;
    
    // 创建数据库管理器
    let db_manager = DatabaseManager::from_config(&config)?;
    
    // 示例1: 创建和存储签名交易数据
    let mut signature_data = SignatureTransactionData::new(
        "5VERv8NMvzbJMEkV8xnrLkEaWRtSz9CosKDYjCJjBRnbJLgp8uirBgmQpjKhoR4tjF3ZpRzrFmBV6UjKdiSZkQUW".to_string(),
        1703875200, // 时间戳
        250000000,  // slot
        true,       // is_successful
    );

    // 添加SOL转账
    signature_data.add_sol_transfer(SolTransfer {
        from: "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v".to_string(),
        to: "7EqQdEULxWcraVx3tXzSFz1hbCqkrvBdBdXkxjt7FuSY".to_string(),
        amount: 1000000000, // 1 SOL
        transfer_type: "系统转账".to_string(),
    });

    // 添加代币转账
    signature_data.add_token_transfer(TokenTransfer {
        from: "7EqQdEULxWcraVx3tXzSFz1hbCqkrvBdBdXkxjt7FuSY".to_string(),
        to: "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v".to_string(),
        amount: 100000000, // 100 USDC (假设)
        decimals: 6,
        mint: "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v".to_string(),
        program_id: "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA".to_string(),
        transfer_type: "代币转账".to_string(),
    });

    // 设置提取的地址信息
    let extracted_addresses = ExtractedAddresses {
        all_addresses: vec![
            "7EqQdEULxWcraVx3tXzSFz1hbCqkrvBdBdXkxjt7FuSY".to_string(),
            "DfXygSm4jCyNCybVYYK6DwvWqjKee8pbDmJGcLWNDXjh".to_string(),
            "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA".to_string(),
        ],
    };
    signature_data.set_extracted_addresses(extracted_addresses);

    // 存储签名数据
    let signature = &signature_data.signature.clone();
    let result = db_manager.signature_storage().store_signature_data(signature, &signature_data)?;
    info!("✅ 存储结果: {}", result.message);

    // 示例2: 查询签名数据
    match db_manager.signature_storage().get_signature_data(signature)? {
        Some(data) => {
            info!("✅ 查询到签名数据:");
            info!("  签名: {}", data.signature);
            info!("  时间戳: {}", data.timestamp);
            info!("  SOL转账数量: {}", data.sol_transfers.len());
            info!("  代币转账数量: {}", data.token_transfers.len());
            info!("  提取地址数量: {}", data.extracted_addresses.all_addresses.len());
        }
        None => {
            info!("❌ 未找到签名数据");
        }
    }

    // 示例3: 检查签名是否存在
    let exists = db_manager.signature_storage().signature_exists(signature)?;
    info!("✅ 签名是否存在: {}", exists);

    // 示例4: 获取统计信息
    let stats = db_manager.signature_storage().get_statistics()?;
    info!("✅ 存储统计信息:");
    info!("  总签名数: {}", stats.total_signatures);
    info!("  SOL转账总数: {}", stats.total_sol_transfers);
    info!("  代币转账总数: {}", stats.total_token_transfers);
    info!("  成功交易数: {}", stats.successful_transactions);
    info!("  失败交易数: {}", stats.failed_transactions);

    // 示例5: 获取所有签名键
    let all_signatures = db_manager.signature_storage().get_all_signature_keys()?;
    info!("✅ 数据库中所有签名数量: {}", all_signatures.len());
    
    // 显示前几个签名作为示例
    for (i, sig) in all_signatures.iter().take(5).enumerate() {
        debug!("  签名 {}: {}", i + 1, sig);
    }

    info!("🎉 数据库示例运行完成！");
    Ok(())
}

/// 演示键前缀的使用
fn demonstrate_key_prefix() -> Result<()> {
    info!("🔑 演示键前缀功能...");

    let config = Config::load()?;
    let storage = StorageManager::new(
        &config.database.db_path,
        config.database.key_prefix_length,
    )?;

    // 演示创建带前缀的键
    let signature = "5VERv8NMvzbJMEkV8xnrLkEaWRtSz9CosKDYjCJjBRnbJLgp8uirBgmQpjKhoR4tjF3ZpRzrFmBV6UjKdiSZkQUW";
    let key = storage.make_key(&config.database.signature_key_prefix, signature)?;
    info!("生成的完整键: {}", key);

    // 演示验证键前缀
    let (prefix, suffix) = storage.validate_key_prefix(&key)?;
    info!("解析的前缀: {}, 后缀: {}", prefix, suffix);

    // 演示错误处理
    match storage.make_key("WRONG", signature) {
        Ok(_) => info!("❌ 不应该成功"),
        Err(e) => info!("✅ 正确捕获错误: {}", e),
    }

    info!("🎉 键前缀演示完成！");
    Ok(())
} 