use anyhow::Result;
use tracing::{error, info};
use tracing_subscriber;

use solana_transfer_ledger::database::test_example::{run_database_example, demonstrate_key_prefix};

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