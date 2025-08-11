use axum::{
    extract::{Path, Query, State},
    response::Json,
};
use serde::Deserialize;
use std::sync::Arc;
use tracing::{info, warn, error};

use crate::database::DatabaseManager;
use super::models::{
    ApiResponse, SignatureQueryResponse, 
    DatabaseStatsResponse, AddressQueryResponse, AddressStatsResponse,
};

/// API 应用状态
pub struct AppState {
    pub db_manager: DatabaseManager,
}

/// 查询参数
#[derive(Debug, Deserialize)]
pub struct QueryParams {
    pub limit: Option<usize>,
    pub offset: Option<usize>,
}

/// 根据签名查询交易数据
#[utoipa::path(
    get,
    path = "/api/v1/transaction/{signature}",
    params(
        ("signature" = String, Path, description = "Transaction signature in base58 format")
    ),
    responses(
        (status = 200, description = "Transaction data found", body = ApiResponse<SignatureQueryResponse>),
        (status = 404, description = "Transaction not found"),
        (status = 400, description = "Invalid signature format"),
        (status = 500, description = "Internal server error")
    ),
    tag = "Transactions"
)]
pub async fn get_transaction_by_signature(
    State(state): State<Arc<AppState>>,
    Path(signature): Path<String>,
) -> Json<ApiResponse<SignatureQueryResponse>> {
    info!("Querying transaction by signature: {}", signature);

    // 验证签名格式
    if signature.is_empty() || signature.len() < 32 {
        warn!("Invalid signature format: {}", signature);
        return Json(ApiResponse::success(
            SignatureQueryResponse {
                signature: "".to_string(),
                sol_transfers: vec![],
                token_transfers: vec![],
                extracted_addresses: Default::default(),
                timestamp: 0,
                slot: 0,
                is_successful: false,
            },
            "Invalid signature format".to_string(),
        ));
    }

    // 查询数据库
    match state.db_manager.signature_storage().get_signature_data(&signature) {
        Ok(Some(data)) => {
            info!("Transaction found for signature: {}", signature);
            let response_data: SignatureQueryResponse = data.into();
            Json(ApiResponse::success(
                response_data,
                "Transaction data retrieved successfully.".to_string(),
            ))
        }
        Ok(None) => {
            info!("Transaction not found for signature: {}", signature);
            Json(ApiResponse::success(
                SignatureQueryResponse {
                    signature: signature.clone(),
                    sol_transfers: vec![],
                    token_transfers: vec![],
                    extracted_addresses: Default::default(),
                    timestamp: 0,
                    slot: 0,
                    is_successful: false,
                },
                "Transaction not found".to_string(),
            ))
        }
        Err(e) => {
            error!("Database error while querying signature {}: {}", signature, e);
            Json(ApiResponse::success(
                SignatureQueryResponse {
                    signature: signature.clone(),
                    sol_transfers: vec![],
                    token_transfers: vec![],
                    extracted_addresses: Default::default(),
                    timestamp: 0,
                    slot: 0,
                    is_successful: false,
                },
                "Database error".to_string(),
            ))
        }
    }
}

/// 获取数据库统计信息
#[utoipa::path(
    get,
    path = "/api/v1/stats",
    responses(
        (status = 200, description = "Database statistics", body = ApiResponse<DatabaseStatsResponse>),
        (status = 500, description = "Internal server error")
    ),
    tag = "Statistics"
)]
pub async fn get_database_stats(
    State(state): State<Arc<AppState>>,
) -> Json<ApiResponse<DatabaseStatsResponse>> {
    info!("Querying database statistics");

    match state.db_manager.signature_storage().get_statistics() {
        Ok(stats) => {
            let response_data = DatabaseStatsResponse {
                total_signatures: stats.total_signatures,
                total_sol_transfers: stats.total_sol_transfers,
                total_token_transfers: stats.total_token_transfers,
                successful_transactions: stats.successful_transactions,
                failed_transactions: stats.failed_transactions,
            };
            Json(ApiResponse::success(
                response_data,
                "Database statistics retrieved successfully.".to_string(),
            ))
        }
        Err(e) => {
            error!("Database error while getting statistics: {}", e);
            Json(ApiResponse::success(
                DatabaseStatsResponse {
                    total_signatures: 0,
                    total_sol_transfers: 0,
                    total_token_transfers: 0,
                    successful_transactions: 0,
                    failed_transactions: 0,
                },
                "Database error".to_string(),
            ))
        }
    }
}

/// 健康检查接口
#[utoipa::path(
    get,
    path = "/api/v1/health",
    responses(
        (status = 200, description = "Service is healthy", body = ApiResponse<String>)
    ),
    tag = "Health"
)]
pub async fn health_check() -> Json<ApiResponse<String>> {
    info!("Health check requested");
    Json(ApiResponse::success(
        "OK".to_string(),
        "Service is running normally.".to_string(),
    ))
}

/// 获取所有签名列表（带分页）
#[utoipa::path(
    get,
    path = "/api/v1/signatures",
    params(
        ("limit" = Option<usize>, Query, description = "Maximum number of signatures to return (default: 100)"),
        ("offset" = Option<usize>, Query, description = "Number of signatures to skip (default: 0)")
    ),
    responses(
        (status = 200, description = "Signatures list", body = ApiResponse<Vec<String>>),
        (status = 500, description = "Internal server error")
    ),
    tag = "Signatures"
)]
pub async fn get_all_signatures(
    State(state): State<Arc<AppState>>,
    Query(params): Query<QueryParams>,
) -> Json<ApiResponse<Vec<String>>> {
    let limit = params.limit.unwrap_or(100).min(1000); // 最大限制1000
    let offset = params.offset.unwrap_or(0);
    
    info!("Querying signatures with limit: {}, offset: {}", limit, offset);

    match state.db_manager.signature_storage().get_all_signature_keys() {
        Ok(mut signatures) => {
            // 应用分页
            let total = signatures.len();
            if offset >= total {
                signatures.clear();
            } else {
                let end = (offset + limit).min(total);
                signatures = signatures[offset..end].to_vec();
            }

            let count = signatures.len();
            info!("Returning {} signatures (total: {})", count, total);
            Json(ApiResponse::success(
                signatures,
                format!("Retrieved {} signatures successfully.", count),
            ))
        }
        Err(e) => {
            error!("Database error while getting signatures: {}", e);
            Json(ApiResponse::success(
                vec![],
                "Database error".to_string(),
            ))
        }
    }
} 

/// 根据地址查询交易记录 / Query transaction records by address
#[utoipa::path(
    get,
    path = "/api/v1/address/{address}/transactions",
    params(
        ("address" = String, Path, description = "Solana地址（base58格式）/ Solana address (base58 format)"),
        ("limit" = Option<usize>, Query, description = "返回记录数量限制，默认100，最大1000 / Limit of returned records, default 100, max 1000"),
        ("offset" = Option<usize>, Query, description = "跳过的记录数量，用于分页，默认0 / Number of records to skip for pagination, default 0")
    ),
    responses(
        (status = 200, description = "查询成功 / Query successful", body = ApiResponse<AddressQueryResponse>),
        (status = 400, description = "地址格式无效 / Invalid address format"),
        (status = 500, description = "服务器内部错误 / Internal server error")
    ),
    tag = "Addresses"
)]
pub async fn get_address_transactions(
    State(state): State<Arc<AppState>>,
    Path(address): Path<String>,
    Query(params): Query<QueryParams>,
) -> Json<ApiResponse<AddressQueryResponse>> {
    info!("查询地址交易记录: {}", address);

    // 验证地址格式
    if address.is_empty() || address.len() < 32 {
        warn!("无效的地址格式: {}", address);
        return Json(ApiResponse::success(
            AddressQueryResponse {
                address: address.clone(),
                total_records: 0,
                records: vec![],
                last_updated: 0,
            },
            "地址格式无效 / Invalid address format".to_string(),
        ));
    }

    let limit = params.limit.unwrap_or(100).min(1000);
    let offset = params.offset.unwrap_or(0);

    // 查询地址交易记录
    match state.db_manager.address_storage().get_address_records(&address) {
        Ok(Some(mut address_list)) => {
            // 应用分页
            let total = address_list.records.len();
            if offset >= total {
                address_list.records.clear();
            } else {
                let end = (offset + limit).min(total);
                address_list.records = address_list.records[offset..end].to_vec();
            }

            info!("找到地址 {} 的 {} 条记录（总共 {} 条）", address, address_list.records.len(), total);
            let response_data: AddressQueryResponse = address_list.into();
            Json(ApiResponse::success(
                response_data,
                format!("成功获取地址交易记录 / Successfully retrieved address transaction records: {} records", total),
            ))
        }
        Ok(None) => {
            info!("地址 {} 没有找到交易记录", address);
            Json(ApiResponse::success(
                AddressQueryResponse {
                    address,
                    total_records: 0,
                    records: vec![],
                    last_updated: 0,
                },
                "该地址没有交易记录 / No transaction records found for this address".to_string(),
            ))
        }
        Err(e) => {
            error!("查询地址 {} 时数据库错误: {}", address, e);
            Json(ApiResponse::success(
                AddressQueryResponse {
                    address,
                    total_records: 0,
                    records: vec![],
                    last_updated: 0,
                },
                "数据库查询错误 / Database query error".to_string(),
            ))
        }
    }
}

/// 获取地址统计信息 / Get address statistics
#[utoipa::path(
    get,
    path = "/api/v1/address/{address}/stats",
    params(
        ("address" = String, Path, description = "Solana地址（base58格式）/ Solana address (base58 format)")
    ),
    responses(
        (status = 200, description = "统计信息获取成功 / Statistics retrieved successfully", body = ApiResponse<AddressStatsResponse>),
        (status = 400, description = "地址格式无效 / Invalid address format"),
        (status = 500, description = "服务器内部错误 / Internal server error")
    ),
    tag = "Addresses"
)]
pub async fn get_address_stats(
    State(state): State<Arc<AppState>>,
    Path(address): Path<String>,
) -> Json<ApiResponse<AddressStatsResponse>> {
    info!("获取地址统计信息: {}", address);

    // 验证地址格式
    if address.is_empty() || address.len() < 32 {
        warn!("无效的地址格式: {}", address);
        return Json(ApiResponse::success(
            AddressStatsResponse {
                address: address.clone(),
                total_records: 0,
                sol_sent_count: 0,
                sol_received_count: 0,
                token_sent_count: 0,
                token_received_count: 0,
                total_sol_sent: 0,
                total_sol_received: 0,
                total_sol_sent_formatted: 0.0,
                total_sol_received_formatted: 0.0,
            },
            "地址格式无效 / Invalid address format".to_string(),
        ));
    }

    // 获取地址统计信息
    match state.db_manager.address_storage().get_address_stats(&address) {
        Ok(stats) => {
            info!("成功获取地址 {} 的统计信息", address);
            let response_data: AddressStatsResponse = stats.into();
            Json(ApiResponse::success(
                response_data,
                "成功获取地址统计信息 / Successfully retrieved address statistics".to_string(),
            ))
        }
        Err(e) => {
            error!("获取地址 {} 统计信息时错误: {}", address, e);
            Json(ApiResponse::success(
                AddressStatsResponse {
                    address,
                    total_records: 0,
                    sol_sent_count: 0,
                    sol_received_count: 0,
                    token_sent_count: 0,
                    token_received_count: 0,
                    total_sol_sent: 0,
                    total_sol_received: 0,
                    total_sol_sent_formatted: 0.0,
                    total_sol_received_formatted: 0.0,
                },
                "获取统计信息失败 / Failed to retrieve statistics".to_string(),
            ))
        }
    }
}

/// 获取所有有记录的地址列表 / Get all addresses with records
#[utoipa::path(
    get,
    path = "/api/v1/addresses",
    params(
        ("limit" = Option<usize>, Query, description = "返回地址数量限制，默认100，最大1000 / Limit of returned addresses, default 100, max 1000"),
        ("offset" = Option<usize>, Query, description = "跳过的地址数量，用于分页，默认0 / Number of addresses to skip for pagination, default 0")
    ),
    responses(
        (status = 200, description = "地址列表获取成功 / Address list retrieved successfully", body = ApiResponse<Vec<String>>),
        (status = 500, description = "服务器内部错误 / Internal server error")
    ),
    tag = "Addresses"
)]
pub async fn get_all_addresses(
    State(state): State<Arc<AppState>>,
    Query(params): Query<QueryParams>,
) -> Json<ApiResponse<Vec<String>>> {
    let limit = params.limit.unwrap_or(100).min(1000);
    let offset = params.offset.unwrap_or(0);
    
    info!("获取地址列表，limit: {}, offset: {}", limit, offset);

    match state.db_manager.address_storage().get_all_addresses() {
        Ok(mut addresses) => {
            // 应用分页
            let total = addresses.len();
            if offset >= total {
                addresses.clear();
            } else {
                let end = (offset + limit).min(total);
                addresses = addresses[offset..end].to_vec();
            }

            let count = addresses.len();
            info!("返回 {} 个地址（总共 {} 个）", count, total);
            Json(ApiResponse::success(
                addresses,
                format!("成功获取地址列表 / Successfully retrieved address list: {} addresses", count),
            ))
        }
        Err(e) => {
            error!("获取地址列表时数据库错误: {}", e);
            Json(ApiResponse::success(
                vec![],
                "数据库错误 / Database error".to_string(),
            ))
        }
    }
} 