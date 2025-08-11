mod config;
mod grpc_client;
mod transfer_parser;
mod address_extractor;
mod database;
mod api;

use anyhow::Result;
use tracing::{error, info};
use tracing_subscriber;
use tokio::signal;

use config::Config;
use grpc_client::SolanaGrpcClient;
use database::DatabaseManager;
use api::ApiServer;

#[tokio::main]
async fn main() -> Result<()> {
    // 初始化日志 - 设置为INFO级别避免过多调试信息
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    info!("🌟 欢迎使用 Solana 地址账本 gRPC 客户端与 API 服务器！");

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

    // 创建数据库管理器
    let db_manager = match DatabaseManager::new(
        &config.database.db_path,
        config.database.signature_key_prefix.clone(),
        config.database.address_key_prefix.clone(),
        config.database.max_address_records,
    ) {
        Ok(db_manager) => {
            info!("✅ 数据库管理器初始化成功");
            db_manager
        }
        Err(e) => {
            error!("❌ 数据库管理器初始化失败: {}", e);
            return Err(e);
        }
    };

    // 创建 gRPC 客户端（带数据库管理器）
    let grpc_client = SolanaGrpcClient::with_database(config.grpc, config.monitor, db_manager.clone());

    // 创建 API 服务器
    let api_server = ApiServer::new(db_manager.clone(), config.api);

    info!("🚀 正在启动服务...");
    info!("📊 gRPC 客户端将监听 Solana 数据并存储到数据库");
    info!("🌐 API 服务器将提供数据查询接口");

    // 使用 tokio::spawn 来并行运行任务，避免阻塞
    let grpc_handle = tokio::spawn(async move {
        info!("🔄 启动 Solana gRPC 数据监听...");
        if let Err(e) = grpc_client.start_monitoring().await {
            error!("❌ gRPC 客户端运行失败: {}", e);
        }
    });

    let api_handle = tokio::spawn(async move {
        info!("🔌 启动 API 服务器...");
        if let Err(e) = api_server.start().await {
            error!("❌ API 服务器运行失败: {}", e);
        }
    });

    // 等待 Ctrl+C 信号
    let ctrl_c = tokio::spawn(async {
        signal::ctrl_c().await.expect("无法监听 Ctrl+C 信号");
        info!("📟 收到 Ctrl+C 信号，正在关闭服务...");
    });

    // 等待任何一个任务完成或收到关闭信号
    tokio::select! {
        _ = grpc_handle => {
            info!("gRPC 客户端已停止");
        }
        _ = api_handle => {
            info!("API 服务器已停止");
        }
        _ = ctrl_c => {
            info!("收到关闭信号");
        }
    }

    info!("🛑 所有服务已停止");
    Ok(())
}
