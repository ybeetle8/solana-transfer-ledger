use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use crate::database::{SignatureTransactionData, SolTransfer, TokenTransfer};

/// API 响应基础结构
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct ApiResponse<T> {
    /// Response status
    pub success: bool,
    /// Response message
    pub message: String,
    /// Response data
    pub data: Option<T>,
    /// Request timestamp
    pub timestamp: i64,
}

/// 错误响应
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct ErrorResponse {
    /// Error message
    pub error: String,
}

/// 签名查询响应数据
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct SignatureQueryResponse {
    /// Transaction signature (base58 encoded)
    pub signature: String,
    /// SOL transfers in this transaction
    pub sol_transfers: Vec<SolTransferResponse>,
    /// Token transfers in this transaction
    pub token_transfers: Vec<TokenTransferResponse>,
    /// Extracted addresses from this transaction
    pub extracted_addresses: ExtractedAddressesResponse,
    /// Transaction timestamp
    pub timestamp: i64,
    /// Block slot number
    pub slot: u64,
    /// Whether transaction was successful
    pub is_successful: bool,
}

/// SOL 转账响应
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct SolTransferResponse {
    /// Sender address
    pub from: String,
    /// Recipient address
    pub to: String,
    /// Transfer amount in lamports
    pub amount: u64,
    /// Transfer amount in SOL (calculated)
    pub amount_sol: f64,
    /// Transfer type description
    pub transfer_type: String,
}

/// 代币转账响应
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct TokenTransferResponse {
    /// Sender address
    pub from: String,
    /// Recipient address
    pub to: String,
    /// Transfer amount (raw)
    pub amount: u64,
    /// Transfer amount (human readable)
    pub amount_formatted: f64,
    /// Token decimals
    pub decimals: u8,
    /// Token mint address
    pub mint: String,
    /// Token program ID
    pub program_id: String,
    /// Transfer type description
    pub transfer_type: String,
}

/// 提取的地址响应
#[derive(Debug, Serialize, Deserialize, ToSchema, Default)]
pub struct ExtractedAddressesResponse {
    /// All addresses involved in the transaction
    pub all_addresses: Vec<String>,
}

/// 签名查询请求
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct SignatureQueryRequest {
    /// Transaction signature in base58 format
    #[schema(example = "5VERv8NMvzbJMEkV8xnrLkEaWRtSz9CosKDYjCJjBRnbJLgp8uirBgmQpjKhoR4tjF3ZpRzrFmBV6UjKdiSZkQUW")]
    pub signature: String,
}

/// 数据库统计响应
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct DatabaseStatsResponse {
    /// Total number of signatures in database
    pub total_signatures: usize,
    /// Total number of SOL transfers
    pub total_sol_transfers: usize,
    /// Total number of token transfers
    pub total_token_transfers: usize,
    /// Number of successful transactions
    pub successful_transactions: usize,
    /// Number of failed transactions
    pub failed_transactions: usize,
}

/// 地址查询响应 / Address Query Response
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct AddressQueryResponse {
    /// 查询的地址 / Queried address
    pub address: String,
    /// 交易记录总数 / Total number of transaction records
    pub total_records: usize,
    /// 交易记录列表（按时间倒序，最新的在前）/ Transaction records list (newest first)
    pub records: Vec<AddressTransactionRecordResponse>,
    /// 最后更新时间戳 / Last updated timestamp
    pub last_updated: u64,
}

/// 地址交易记录响应 / Address Transaction Record Response
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct AddressTransactionRecordResponse {
    /// 交易签名 / Transaction signature
    pub signature: String,
    /// 交易时间戳 / Transaction timestamp
    pub timestamp: u64,
    /// 交易槽位 / Transaction slot
    pub slot: u64,
    /// SOL转账记录（如果有）/ SOL transfer record (if any)
    pub sol_transfer: Option<SolTransferResponse>,
    /// 代币转账记录（如果有）/ Token transfer record (if any)
    pub token_transfer: Option<TokenTransferResponse>,
    /// 记录类型：发送方或接收方 / Record type: sender or receiver
    pub record_type: String,
}

/// 地址统计信息响应 / Address Statistics Response
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct AddressStatsResponse {
    /// 查询的地址 / Queried address
    pub address: String,
    /// 总记录数 / Total number of records
    pub total_records: usize,
    /// SOL发送次数 / Number of SOL sent transactions
    pub sol_sent_count: usize,
    /// SOL接收次数 / Number of SOL received transactions
    pub sol_received_count: usize,
    /// 代币发送次数 / Number of token sent transactions
    pub token_sent_count: usize,
    /// 代币接收次数 / Number of token received transactions
    pub token_received_count: usize,
    /// 总SOL发送数量（lamports）/ Total SOL sent amount (lamports)
    pub total_sol_sent: u64,
    /// 总SOL接收数量（lamports）/ Total SOL received amount (lamports)
    pub total_sol_received: u64,
    /// 总SOL发送数量（SOL）/ Total SOL sent amount (SOL)
    pub total_sol_sent_formatted: f64,
    /// 总SOL接收数量（SOL）/ Total SOL received amount (SOL)
    pub total_sol_received_formatted: f64,
}

impl<T> ApiResponse<T> {
    /// Create success response
    pub fn success(data: T, message: String) -> Self {
        Self {
            success: true,
            message,
            data: Some(data),
            timestamp: chrono::Utc::now().timestamp(),
        }
    }

    /// Create error response
    pub fn error(message: String) -> ApiResponse<ErrorResponse> {
        ApiResponse {
            success: false,
            message: message.clone(),
            data: Some(ErrorResponse { 
                error: message,
            }),
            timestamp: chrono::Utc::now().timestamp(),
        }
    }
}

// 类型转换实现

impl From<crate::database::signature_storage::SignatureTransactionData> for SignatureQueryResponse {
    fn from(data: crate::database::signature_storage::SignatureTransactionData) -> Self {
        Self {
            signature: data.signature,
            sol_transfers: data.sol_transfers.into_iter().map(Into::into).collect(),
            token_transfers: data.token_transfers.into_iter().map(Into::into).collect(),
            extracted_addresses: data.extracted_addresses.into(),
            timestamp: data.timestamp,
            slot: data.slot,
            is_successful: data.is_successful,
        }
    }
}

impl From<crate::database::signature_storage::SolTransfer> for SolTransferResponse {
    fn from(data: crate::database::signature_storage::SolTransfer) -> Self {
        Self {
            from: data.from,
            to: data.to,
            amount: data.amount,
            amount_sol: data.amount as f64 / 1_000_000_000.0,
            transfer_type: data.transfer_type,
        }
    }
}

impl From<crate::database::signature_storage::TokenTransfer> for TokenTransferResponse {
    fn from(data: crate::database::signature_storage::TokenTransfer) -> Self {
        Self {
            from: data.from,
            to: data.to,
            amount: data.amount,
            amount_formatted: data.amount as f64 / 10_f64.powi(data.decimals as i32),
            decimals: data.decimals,
            mint: data.mint,
            program_id: data.program_id,
            transfer_type: data.transfer_type,
        }
    }
}

impl From<crate::database::signature_storage::ExtractedAddresses> for ExtractedAddressesResponse {
    fn from(data: crate::database::signature_storage::ExtractedAddresses) -> Self {
        Self {
            all_addresses: data.all_addresses,
        }
    }
}

impl From<crate::database::address_storage::AddressTransactionRecord> for AddressTransactionRecordResponse {
    fn from(record: crate::database::address_storage::AddressTransactionRecord) -> Self {
        Self {
            signature: record.signature,
            timestamp: record.timestamp,
            slot: record.slot,
            sol_transfer: record.sol_transfer.map(|st| SolTransferResponse {
                from: st.from,
                to: st.to,
                amount: st.amount,
                amount_sol: st.amount as f64 / 1_000_000_000.0,
                transfer_type: st.transfer_type,
            }),
            token_transfer: record.token_transfer.map(|tt| TokenTransferResponse {
                from: tt.from,
                to: tt.to,
                amount: tt.amount,
                amount_formatted: tt.amount as f64 / 10_f64.powi(tt.decimals as i32),
                decimals: tt.decimals as u8,
                mint: tt.mint,
                program_id: tt.program_id,
                transfer_type: tt.transfer_type,
            }),
            record_type: match record.record_type {
                crate::database::address_storage::RecordType::Sender => "sender".to_string(),
                crate::database::address_storage::RecordType::Receiver => "receiver".to_string(),
            },
        }
    }
}

impl From<crate::database::address_storage::AddressTransactionList> for AddressQueryResponse {
    fn from(list: crate::database::address_storage::AddressTransactionList) -> Self {
        Self {
            address: list.address,
            total_records: list.records.len(),
            records: list.records.into_iter().map(Into::into).collect(),
            last_updated: list.last_updated,
        }
    }
}

impl From<crate::database::address_storage::AddressStats> for AddressStatsResponse {
    fn from(stats: crate::database::address_storage::AddressStats) -> Self {
        Self {
            address: stats.address,
            total_records: stats.total_records,
            sol_sent_count: stats.sol_sent_count,
            sol_received_count: stats.sol_received_count,
            token_sent_count: stats.token_sent_count,
            token_received_count: stats.token_received_count,
            total_sol_sent: stats.total_sol_sent,
            total_sol_received: stats.total_sol_received,
            total_sol_sent_formatted: stats.total_sol_sent as f64 / 1_000_000_000.0,
            total_sol_received_formatted: stats.total_sol_received as f64 / 1_000_000_000.0,
        }
    }
} 