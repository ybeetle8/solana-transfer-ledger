use axum::{
    extract::DefaultBodyLimit,
    routing::get,
    Router,
};
use std::sync::Arc;
use tower::ServiceBuilder;
use tower_http::{
    cors::CorsLayer,
    trace::TraceLayer,
};
use tracing::info;
use utoipa::OpenApi;
use utoipa_swagger_ui::SwaggerUi;

use crate::config::ApiConfig;
use crate::database::DatabaseManager;
use super::handlers::{
    AppState, get_transaction_by_signature,
    get_database_stats, health_check, get_all_signatures,
    get_address_transactions, get_address_stats, get_all_addresses,
};
use super::models::{
    ApiResponse, ErrorResponse, SignatureQueryResponse, SignatureQueryRequest,
    DatabaseStatsResponse, SolTransferResponse,
    TokenTransferResponse, ExtractedAddressesResponse,
    AddressQueryResponse, AddressStatsResponse, AddressTransactionRecordResponse,
};

/// API 文档结构
#[derive(OpenApi)]
#[openapi(
    paths(
        super::handlers::get_transaction_by_signature,
        super::handlers::get_database_stats,
        super::handlers::health_check,
        super::handlers::get_all_signatures,
        super::handlers::get_address_transactions,
        super::handlers::get_address_stats,
        super::handlers::get_all_addresses,
    ),
    components(
        schemas(
            ApiResponse<SignatureQueryResponse>,
            ApiResponse<ErrorResponse>,
            ApiResponse<DatabaseStatsResponse>,
            ApiResponse<Vec<String>>,
            ApiResponse<String>,
            ApiResponse<AddressQueryResponse>,
            ApiResponse<AddressStatsResponse>,
            SignatureQueryResponse,
            ErrorResponse,
            SignatureQueryRequest,
            DatabaseStatsResponse,
            SolTransferResponse,
            TokenTransferResponse,
            ExtractedAddressesResponse,
            AddressQueryResponse,
            AddressStatsResponse,
            AddressTransactionRecordResponse,
        )
    ),
    tags(
        (name = "Transactions", description = "Transaction query endpoints"),
        (name = "Addresses", description = "Address-related query endpoints"),
        (name = "Signatures", description = "Signature management endpoints"),
        (name = "Statistics", description = "Database statistics endpoints"),
        (name = "Health", description = "Health check endpoints")
    ),
    info(
        title = "Solana Transfer Ledger API",
        version = "1.0.0",
        description = "API for querying Solana transaction data stored in RocksDB",
        contact(
            name = "API Support",
            email = "support@example.com"
        ),
        license(
            name = "MIT",
            url = "https://opensource.org/licenses/MIT"
        )
    )
)]
pub struct ApiDoc;

/// API 服务器
pub struct ApiServer {
    db_manager: DatabaseManager,
    config: ApiConfig,
}

impl ApiServer {
    /// 创建新的 API 服务器
    pub fn new(db_manager: DatabaseManager, config: ApiConfig) -> Self {
        Self { db_manager, config }
    }

    /// 创建应用路由
    pub fn create_app(&self) -> Router {
        let state = Arc::new(AppState {
            db_manager: self.db_manager.clone(),
        });

        // 创建 API 路由
        let api_routes = Router::new()
            .route("/health", get(health_check))
            .route("/transaction/:signature", get(get_transaction_by_signature))
            .route("/signatures", get(get_all_signatures))
            .route("/stats", get(get_database_stats))
            .route("/addresses", get(get_all_addresses))
            .route("/address/:address/transactions", get(get_address_transactions))
            .route("/address/:address/stats", get(get_address_stats));

        // 主路由
        let app = Router::new()
            .nest("/api/v1", api_routes)
            // Swagger UI
            .merge(SwaggerUi::new("/docs").url("/api-docs/openapi.json", ApiDoc::openapi()))
            .with_state(state)
            .layer(
                ServiceBuilder::new()
                    .layer(TraceLayer::new_for_http())
                    .layer(if self.config.enable_cors {
                        CorsLayer::permissive()
                    } else {
                        CorsLayer::new()
                    })
                    .layer(DefaultBodyLimit::max(1024 * 1024)) // 1MB
            );

        app
    }

    /// 启动服务器
    pub async fn start(&self) -> anyhow::Result<()> {
        let app = self.create_app();
        let addr = format!("{}:{}", self.config.host, self.config.port);
        
        info!("🚀 Starting API server on {}", addr);
        info!("📚 Swagger documentation available at: http://{}/docs", addr);
        info!("🔍 API endpoints:");
        info!("  GET  /api/v1/health                        - Health check");
        info!("  GET  /api/v1/transaction/{{signature}}       - Get transaction by signature");
        info!("  GET  /api/v1/signatures                     - Get all signatures (paginated)");
        info!("  GET  /api/v1/stats                          - Get database statistics");
        info!("  GET  /api/v1/addresses                      - Get all addresses with records");
        info!("  GET  /api/v1/address/{{address}}/transactions - Get transactions by address");
        info!("  GET  /api/v1/address/{{address}}/stats       - Get address statistics");

        let listener = tokio::net::TcpListener::bind(&addr).await?;
        axum::serve(listener, app).await?;

        Ok(())
    }
} 