mod config;
mod grpc_client;
mod transfer_parser;

use anyhow::Result;
use tracing::{error, info};
use tracing_subscriber;

use config::Config;
use grpc_client::SolanaGrpcClient;

#[tokio::main]
async fn main() -> Result<()> {
    // 初始化日志 - 设置为DEBUG级别以查看代币转账调试信息
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .init();

    info!("🌟 欢迎使用 Solana 地址账本 gRPC 客户端！");

    // 加载配置
    let config = match Config::load() {
        Ok(config) => {
            info!("✅ 成功加载配置文件");
            config
        }
        Err(e) => {
            error!("❌ 加载配置文件失败: {}", e);
            error!("请确保项目根目录下存在 config.toml 文件");
            return Err(e);
        }
    };

    // 创建 gRPC 客户端
    let client = SolanaGrpcClient::new(config.grpc, config.monitor);

    info!("🚀 开始启动 Solana gRPC 数据监听...");

    // 开始监听数据
    if let Err(e) = client.start_monitoring().await {
        error!("❌ gRPC 客户端运行失败: {}", e);
        return Err(e);
    }

    Ok(())
}
