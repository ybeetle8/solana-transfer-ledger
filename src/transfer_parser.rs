use anyhow::Result;
use tracing::{debug, warn};
use std::collections::HashMap;
use yellowstone_grpc_proto::prelude::{
    SubscribeUpdateTransaction, TransactionStatusMeta, Message
};
use yellowstone_grpc_proto::solana::storage::confirmed_block::TokenBalance;

/// 控制是否显示详细调试信息
const SHOW_DEBUG_INFO: bool = false;

/// SOL转账记录
#[derive(Debug, Clone)]
pub struct SolTransfer {
    /// 交易签名
    pub signature: String,
    /// 转出方账户地址
    pub from: String,
    /// 接收方账户地址
    pub to: String,
    /// 转账金额（lamports单位）
    pub amount: u64,
    /// 转出方账户索引
    pub from_index: usize,
    /// 接收方账户索引
    pub to_index: usize,
    /// 交易时间戳（秒级）
    pub timestamp: u32,
}

/// 代币转账记录
#[derive(Debug, Clone)]
pub struct TokenTransfer {
    /// 交易签名
    pub signature: String,
    /// 转出方账户地址
    pub from: String,
    /// 接收方账户地址
    pub to: String,
    /// 转账金额（最小代币单位）
    pub amount: u64,
    /// 代币mint地址
    pub mint: String,
    /// 代币小数位数
    pub decimals: u32,
    /// 交易时间戳（秒级）
    pub timestamp: u32,
}

/// 账户余额变化信息
#[derive(Debug, Clone)]
struct AccountBalanceChange {
    /// 账户索引
    index: usize,
    /// 账户地址
    address: String,
    /// 余额变化（lamports，正数表示增加，负数表示减少）
    change: i64,
    /// 执行前余额
    pre_balance: u64,
    /// 执行后余额
    post_balance: u64,
}

/// 转账解析器
pub struct TransferParser;

impl TransferParser {
    /// 解析交易中的SOL转账
    /// 
    /// # 参数
    /// - `transaction_update`: 交易更新数据
    /// - `timestamp`: 交易时间戳（秒级）
    /// 
    /// # 返回
    /// 返回解析出的所有SOL转账记录
    pub fn parse_sol_transfers(transaction_update: &SubscribeUpdateTransaction, timestamp: u32) -> Result<Vec<SolTransfer>> {
        let Some(tx_info) = &transaction_update.transaction else {
            debug!("交易信息为空，跳过解析");
            return Ok(vec![]);
        };

        let Some(meta) = &tx_info.meta else {
            debug!("交易元数据为空，跳过解析");
            return Ok(vec![]);
        };

        let Some(raw_tx) = &tx_info.transaction else {
            debug!("原始交易数据为空，跳过解析");
            return Ok(vec![]);
        };

        let Some(message) = &raw_tx.message else {
            debug!("交易消息为空，跳过解析");
            return Ok(vec![]);
        };

        // 获取完整的账户地址列表
        let account_addresses = Self::build_complete_account_list(message, meta)?;
        
        // 分析余额变化
        let balance_changes = Self::analyze_balance_changes(&account_addresses, meta)?;
        
        // 解析转账
        let transfers = Self::extract_transfers(&balance_changes, &tx_info.signature, timestamp)?;
        
        Ok(transfers)
    }

    /// 解析交易中的代币转账
    /// 
    /// # 参数
    /// - `transaction_update`: 交易更新数据
    /// - `timestamp`: 交易时间戳（秒级）
    /// 
    /// # 返回
    /// 返回解析出的所有代币转账记录
    pub fn parse_token_transfers(transaction_update: &SubscribeUpdateTransaction, timestamp: u32) -> Result<Vec<TokenTransfer>> {
        let Some(tx_info) = &transaction_update.transaction else {
            debug!("交易信息为空，跳过代币转账解析");
            return Ok(vec![]);
        };

        let signature_str = bs58::encode(&tx_info.signature).into_string();
        debug!("开始解析代币转账，签名: {}", signature_str);

        let Some(meta) = &tx_info.meta else {
            debug!("交易元数据为空，跳过代币转账解析，签名: {}", signature_str);
            return Ok(vec![]);
        };

        let Some(raw_tx) = &tx_info.transaction else {
            debug!("原始交易数据为空，跳过代币转账解析，签名: {}", signature_str);
            return Ok(vec![]);
        };

        let Some(message) = &raw_tx.message else {
            debug!("交易消息为空，跳过代币转账解析，签名: {}", signature_str);
            return Ok(vec![]);
        };

        // 获取完整的账户地址列表
        let account_addresses = Self::build_complete_account_list(message, meta)?;
        
        debug!("代币余额信息，签名: {} - 执行前: {} 个, 执行后: {} 个", 
               signature_str, meta.pre_token_balances.len(), meta.post_token_balances.len());

        // 如果没有代币余额变化，直接返回
        if meta.pre_token_balances.is_empty() && meta.post_token_balances.is_empty() {
            debug!("无代币余额变化，签名: {}", signature_str);
            return Ok(vec![]);
        }
        
        // 分析代币余额变化
        let token_transfers = Self::analyze_token_balance_changes(
            &account_addresses, 
            &meta.pre_token_balances, 
            &meta.post_token_balances, 
            &tx_info.signature,
            timestamp
        )?;
        
        Ok(token_transfers)
    }

    /// 构建完整的账户地址列表
    /// 
    /// 将 accountKeys 和通过地址查找表加载的地址合并
    fn build_complete_account_list(message: &Message, meta: &TransactionStatusMeta) -> Result<Vec<String>> {
        let mut addresses = Vec::new();
        
        // 添加直接存储的账户地址
        for account_key in &message.account_keys {
            addresses.push(bs58::encode(account_key).into_string());
        }
        
        // 添加通过地址查找表加载的可写地址
        for address in &meta.loaded_writable_addresses {
            addresses.push(bs58::encode(address).into_string());
        }
        
        // 添加通过地址查找表加载的只读地址
        for address in &meta.loaded_readonly_addresses {
            addresses.push(bs58::encode(address).into_string());
        }
        
        debug!("构建完整账户地址列表: {} 个账户", addresses.len());
        Ok(addresses)
    }

    /// 分析账户余额变化
    fn analyze_balance_changes(
        account_addresses: &[String],
        meta: &TransactionStatusMeta,
    ) -> Result<Vec<AccountBalanceChange>> {
        if meta.pre_balances.len() != meta.post_balances.len() {
            warn!(
                "前后余额数组长度不一致: pre={}, post={}",
                meta.pre_balances.len(),
                meta.post_balances.len()
            );
            return Ok(vec![]);
        }

        if account_addresses.len() < meta.pre_balances.len() {
            warn!(
                "账户地址数量不足: addresses={}, balances={}",
                account_addresses.len(),
                meta.pre_balances.len()
            );
            return Ok(vec![]);
        }

        let mut changes = Vec::new();

        for (index, (pre_balance, post_balance)) in meta
            .pre_balances
            .iter()
            .zip(meta.post_balances.iter())
            .enumerate()
        {
            let change = *post_balance as i64 - *pre_balance as i64;
            
            // 只记录有余额变化的账户
            if change != 0 {
                let address = account_addresses
                    .get(index)
                    .map(|s| s.clone())
                    .unwrap_or_else(|| format!("unknown_{}", index));

                changes.push(AccountBalanceChange {
                    index,
                    address,
                    change,
                    pre_balance: *pre_balance,
                    post_balance: *post_balance,
                });
            }
        }

        debug!("发现 {} 个账户有余额变化", changes.len());
        Ok(changes)
    }

    /// 从余额变化中提取转账信息
    fn extract_transfers(
        balance_changes: &[AccountBalanceChange],
        signature: &[u8],
        timestamp: u32,
    ) -> Result<Vec<SolTransfer>> {
        let signature_str = bs58::encode(signature).into_string();
        let mut transfers = Vec::new();

        // 分离转出方和转入方
        let senders: Vec<&AccountBalanceChange> = balance_changes
            .iter()
            .filter(|change| change.change < 0)
            .collect();

        let receivers: Vec<&AccountBalanceChange> = balance_changes
            .iter()
            .filter(|change| change.change > 0)
            .collect();

        if SHOW_DEBUG_INFO {
            debug!(
                "发现 {} 个转出方，{} 个接收方",
                senders.len(),
                receivers.len()
            );
        }

        // 如果只有转出方而没有接收方，可能只是支付了gas费用，不算转账
        if receivers.is_empty() {
            if SHOW_DEBUG_INFO {
                debug!("没有发现接收方，可能只是gas费用消耗");
            }
            return Ok(transfers);
        }

        // 改进的转账匹配逻辑：支持一对多、多对一的情况
        let mut used_senders = vec![false; senders.len()];
        let mut used_receivers = vec![false; receivers.len()];
        
        // 1. 首先尝试精确匹配（金额完全相等或非常接近）
        for (i, sender) in senders.iter().enumerate() {
            if used_senders[i] {
                continue;
            }
            
            let send_amount = (-sender.change) as u64;
            
            for (j, receiver) in receivers.iter().enumerate() {
                if used_receivers[j] {
                    continue;
                }
                
                let receive_amount = receiver.change as u64;
                
                // 精确匹配：允许5%的误差（考虑手续费）
                if Self::is_matching_transfer(send_amount, receive_amount) {
                    transfers.push(SolTransfer {
                        signature: signature_str.clone(),
                        from: sender.address.clone(),
                        to: receiver.address.clone(),
                        amount: receive_amount,
                        from_index: sender.index,
                        to_index: receiver.index,
                        timestamp,
                    });

                    used_senders[i] = true;
                    used_receivers[j] = true;

                    if SHOW_DEBUG_INFO {
                        debug!(
                            "精确匹配转账: {} -> {} ({:.9} SOL)",
                            &sender.address[..8],
                            &receiver.address[..8],
                            receive_amount as f64 / 1_000_000_000.0
                        );
                    }
                    break;
                }
            }
        }
        
        // 2. 处理剩余的发送方：一对多情况（一个发送方对应多个接收方）
        for (i, sender) in senders.iter().enumerate() {
            if used_senders[i] {
                continue;
            }
            
            let send_amount = (-sender.change) as u64;
            let mut remaining_amount = send_amount;
            
            // 收集可能的接收方
            let mut candidate_receivers = Vec::new();
            for (j, receiver) in receivers.iter().enumerate() {
                if !used_receivers[j] {
                    let receive_amount = receiver.change as u64;
                    // 接收金额不能超过发送金额的150%（考虑可能的利息、奖励等）
                    if receive_amount <= send_amount * 15 / 10 && receive_amount >= 100_000 { // 至少0.0001 SOL
                        candidate_receivers.push((j, receiver, receive_amount));
                    }
                }
            }
            
            // 按接收金额从大到小排序
            candidate_receivers.sort_by(|a, b| b.2.cmp(&a.2));
            
            // 贪心匹配：尽量用完发送金额
            for (j, receiver, receive_amount) in candidate_receivers {
                if remaining_amount < 100_000 { // 剩余金额太少就停止
                    break;
                }
                
                if receive_amount <= remaining_amount * 11 / 10 { // 允许10%的超出（手续费等）
                    transfers.push(SolTransfer {
                        signature: signature_str.clone(),
                        from: sender.address.clone(),
                        to: receiver.address.clone(),
                        amount: receive_amount,
                        from_index: sender.index,
                        to_index: receiver.index,
                        timestamp,
                    });

                    used_receivers[j] = true;
                    remaining_amount = remaining_amount.saturating_sub(receive_amount);

                    if SHOW_DEBUG_INFO {
                        debug!(
                            "一对多转账: {} -> {} ({:.9} SOL, 剩余{:.9} SOL)",
                            &sender.address[..8],
                            &receiver.address[..8],
                            receive_amount as f64 / 1_000_000_000.0,
                            remaining_amount as f64 / 1_000_000_000.0
                        );
                    }
                }
            }
            
            if remaining_amount < send_amount / 2 { // 如果匹配了超过一半的金额，标记为已使用
                used_senders[i] = true;
            }
        }
        
        // 3. 处理剩余的接收方：多对一情况（多个发送方对应一个接收方）
        for (j, receiver) in receivers.iter().enumerate() {
            if used_receivers[j] {
                continue;
            }
            
            let receive_amount = receiver.change as u64;
            let mut remaining_needed = receive_amount;
            
            // 收集可能的发送方
            let mut candidate_senders = Vec::new();
            for (i, sender) in senders.iter().enumerate() {
                if !used_senders[i] {
                    let send_amount = (-sender.change) as u64;
                    if send_amount >= 100_000 { // 至少0.0001 SOL
                        candidate_senders.push((i, sender, send_amount));
                    }
                }
            }
            
            // 按发送金额从大到小排序
            candidate_senders.sort_by(|a, b| b.2.cmp(&a.2));
            
            // 尝试用多个发送方组合成这个接收金额
            for (i, sender, send_amount) in candidate_senders {
                if remaining_needed < 100_000 {
                    break;
                }
                
                let used_amount = send_amount.min(remaining_needed * 11 / 10); // 允许10%超出
                
                transfers.push(SolTransfer {
                    signature: signature_str.clone(),
                    from: sender.address.clone(),
                    to: receiver.address.clone(),
                    amount: used_amount.min(remaining_needed),
                    from_index: sender.index,
                    to_index: receiver.index,
                    timestamp,
                });

                remaining_needed = remaining_needed.saturating_sub(used_amount.min(remaining_needed));

                                    if SHOW_DEBUG_INFO {
                        debug!(
                            "多对一转账: {} -> {} ({:.9} SOL, 还需{:.9} SOL)",
                            &sender.address[..8],
                            &receiver.address[..8],
                            used_amount.min(remaining_needed) as f64 / 1_000_000_000.0,
                            remaining_needed as f64 / 1_000_000_000.0
                        );
                    }
                
                // 如果这个发送方的大部分金额都被使用了，标记为已使用
                if used_amount >= send_amount * 8 / 10 {
                    used_senders[i] = true;
                }
            }
            
            if remaining_needed < receive_amount / 2 { // 如果匹配了超过一半的金额，标记为已使用
                used_receivers[j] = true;
            }
        }
        
        // 4. 处理完全无法匹配的情况：记录所有剩余的显著变化
        for (j, receiver) in receivers.iter().enumerate() {
            if !used_receivers[j] && receiver.change > 1_000_000 { // 超过0.001 SOL
                // 寻找任意一个未完全使用的发送方
                if let Some((i, sender)) = senders.iter().enumerate()
                    .find(|(i, s)| !used_senders[*i] && (-s.change) as u64 > 100_000) {
                    
                    transfers.push(SolTransfer {
                        signature: signature_str.clone(),
                        from: sender.address.clone(),
                        to: receiver.address.clone(),
                        amount: receiver.change as u64,
                        from_index: sender.index,
                        to_index: receiver.index,
                        timestamp,
                    });

                    if SHOW_DEBUG_INFO {
                        debug!(
                            "推测转账: {} -> {} ({:.9} SOL)",
                            &sender.address[..8],
                            &receiver.address[..8],
                            receiver.change as f64 / 1_000_000_000.0
                        );
                    }
                }
            }
        }

        Ok(transfers)
    }

    /// 判断两个余额变化是否为匹配的转账对
    /// 
    /// 考虑到gas费用的影响，允许一定的偏差
    fn is_matching_transfer(send_amount: u64, receive_amount: u64) -> bool {
        // 完全匹配
        if send_amount == receive_amount {
            return true;
        }

        // 发送金额大于接收金额（考虑gas费用）
        // 允许的gas费用范围：最多0.01 SOL
        const MAX_GAS_FEE: u64 = 10_000_000; // 0.01 SOL in lamports
        
        if send_amount > receive_amount && (send_amount - receive_amount) <= MAX_GAS_FEE {
            return true;
        }

        // 对于大额转账，允许更大的gas费用偏差（但比例不超过1%）
        if send_amount > receive_amount {
            let difference = send_amount - receive_amount;
            let max_allowed_diff = (send_amount / 100).max(MAX_GAS_FEE); // 最大1%或0.01 SOL
            return difference <= max_allowed_diff;
        }

        false
    }

    /// 分析代币余额变化
    fn analyze_token_balance_changes(
        account_addresses: &[String],
        pre_token_balances: &[TokenBalance],
        post_token_balances: &[TokenBalance],
        signature: &[u8],
        timestamp: u32,
    ) -> Result<Vec<TokenTransfer>> {
        let signature_str = bs58::encode(signature).into_string();
        let mut transfers = Vec::new();

        if SHOW_DEBUG_INFO {
            debug!("分析代币余额变化，签名: {}, pre: {}, post: {}", 
                   signature_str, pre_token_balances.len(), post_token_balances.len());

            // 打印所有代币余额信息用于调试
            for (i, balance) in pre_token_balances.iter().enumerate() {
                debug!("Pre[{}]: 账户索引={}, mint={}, amount={:?}", 
                       i, balance.account_index, balance.mint, 
                       balance.ui_token_amount.as_ref().map(|a| &a.amount));
            }
            
            for (i, balance) in post_token_balances.iter().enumerate() {
                debug!("Post[{}]: 账户索引={}, mint={}, amount={:?}", 
                       i, balance.account_index, balance.mint, 
                       balance.ui_token_amount.as_ref().map(|a| &a.amount));
            }
        }

        // 创建映射表便于比较
        let pre_map: HashMap<(u32, String), &TokenBalance> = pre_token_balances
            .iter()
            .map(|tb| ((tb.account_index, tb.mint.clone()), tb))
            .collect();

        let post_map: HashMap<(u32, String), &TokenBalance> = post_token_balances
            .iter()
            .map(|tb| ((tb.account_index, tb.mint.clone()), tb))
            .collect();

        // 收集所有发生变化的账户
        let mut balance_changes: Vec<(u32, String, i64, u32)> = Vec::new(); // (account_index, mint, change, decimals)

        // 分析现有账户的变化
        for ((account_index, mint), post_balance) in &post_map {
            if let Some(pre_balance) = pre_map.get(&(*account_index, mint.clone())) {
                // 检查是否为同一种代币
                if pre_balance.mint == post_balance.mint {
                    if let (Some(pre_amount), Some(post_amount)) = 
                        (&pre_balance.ui_token_amount, &post_balance.ui_token_amount) {
                        
                        // 解析金额
                        let pre_raw: Result<u64, _> = pre_amount.amount.parse();
                        let post_raw: Result<u64, _> = post_amount.amount.parse();
                        
                        if let (Ok(pre_raw), Ok(post_raw)) = (pre_raw, post_raw) {
                            if pre_raw != post_raw {
                                let change = post_raw as i64 - pre_raw as i64;
                                
                                // 记录所有变化（不管正负）
                                if change != 0 {
                                    balance_changes.push((*account_index, mint.clone(), change, post_amount.decimals));
                                    if SHOW_DEBUG_INFO {
                                        debug!("余额变化: 账户{}，代币{}，变化{}", 
                                               account_index, &mint[..8], change);
                                    }
                                }
                            }
                        }
                    }
                }
            } else {
                // 新创建的代币账户
                if let Some(post_amount) = &post_balance.ui_token_amount {
                    let post_raw: Result<u64, _> = post_amount.amount.parse();
                    if let Ok(post_raw) = post_raw {
                        if post_raw > 0 {
                            balance_changes.push((*account_index, mint.clone(), post_raw as i64, post_amount.decimals));
                            if SHOW_DEBUG_INFO {
                                debug!("新账户接收: 账户{}，代币{}，金额{}", 
                                       account_index, &mint[..8], post_raw);
                            }
                        }
                    }
                }
            }
        }

        // 检查在post中消失的账户（代币账户被关闭）
        for ((account_index, mint), pre_balance) in &pre_map {
            if !post_map.contains_key(&(*account_index, mint.clone())) {
                if let Some(pre_amount) = &pre_balance.ui_token_amount {
                    let pre_raw: Result<u64, _> = pre_amount.amount.parse();
                    if let Ok(pre_raw) = pre_raw {
                        if pre_raw > 0 {
                            balance_changes.push((*account_index, mint.clone(), -(pre_raw as i64), pre_amount.decimals));
                            if SHOW_DEBUG_INFO {
                                debug!("账户关闭: 账户{}，代币{}，失去{}", 
                                       account_index, &mint[..8], pre_raw);
                            }
                        }
                    }
                }
            }
        }

        // 按mint分组处理转账
        let mut mint_groups: HashMap<String, Vec<(u32, i64, u32)>> = HashMap::new();
        for (account_index, mint, change, decimals) in balance_changes {
            mint_groups.entry(mint).or_insert_with(Vec::new)
                .push((account_index, change, decimals));
        }

        // 为每种代币寻找转账对
        for (mint, changes) in mint_groups {
            // 分离增加和减少的账户
            let increases: Vec<&(u32, i64, u32)> = changes.iter().filter(|(_, change, _)| *change > 0).collect();
            let decreases: Vec<&(u32, i64, u32)> = changes.iter().filter(|(_, change, _)| *change < 0).collect();

            if SHOW_DEBUG_INFO {
                debug!("代币 {}: {} 个增加, {} 个减少", &mint[..8], increases.len(), decreases.len());
            }

            // 简单情况：一对一转账
            if increases.len() == 1 && decreases.len() == 1 {
                let (to_index, to_change, decimals) = increases[0];
                let (from_index, from_change, _) = decreases[0];
                
                // 检查金额是否大致匹配（非常宽松的条件）
                let to_amount = *to_change as u64;
                let from_amount = (-from_change) as u64;
                
                // 允许最多10倍的误差（考虑复杂的DeFi操作、手续费、slippage等）
                if to_amount >= (from_amount / 10) && to_amount <= (from_amount * 10) {
                    let from_address = account_addresses
                        .get(*from_index as usize)
                        .map(|s| s.clone())
                        .unwrap_or_else(|| format!("unknown_{}", from_index));
                    
                    let to_address = account_addresses
                        .get(*to_index as usize)
                        .map(|s| s.clone())
                        .unwrap_or_else(|| format!("unknown_{}", to_index));

                    // 使用实际转入的金额作为转账金额
                    transfers.push(TokenTransfer {
                        signature: signature_str.clone(),
                        from: from_address.clone(),
                        to: to_address.clone(),
                        amount: to_amount,
                        mint: mint.clone(),
                        decimals: *decimals,
                        timestamp,
                    });

                    if SHOW_DEBUG_INFO {
                        debug!("发现代币转账: {} -> {} ({} {} tokens, 比例{:.2})",
                               &from_address[..8], &to_address[..8], to_amount, &mint[..8], 
                               to_amount as f64 / from_amount as f64);
                    }
                }
            }
            // 复杂情况：多对多，尝试贪心匹配
            else if !increases.is_empty() && !decreases.is_empty() {
                let mut used_decreases = vec![false; decreases.len()];
                
                for (to_index, to_change, decimals) in &increases {
                    let to_amount = *to_change as u64;
                    
                    // 寻找最匹配的减少
                    let mut best_match = None;
                    let mut best_ratio = f64::INFINITY;
                    
                    for (i, (from_index, from_change, _)) in decreases.iter().enumerate() {
                        if used_decreases[i] {
                            continue;
                        }
                        
                        let from_amount = (-from_change) as u64;
                        let ratio = if from_amount > to_amount {
                            from_amount as f64 / to_amount as f64
                        } else {
                            to_amount as f64 / from_amount as f64
                        };
                        
                        // 允许最多10倍的差异（非常宽松）
                        if ratio <= 10.0 && ratio < best_ratio {
                            best_ratio = ratio;
                            best_match = Some((i, *from_index, from_amount));
                        }
                    }
                    
                    if let Some((decrease_idx, from_index, from_amount)) = best_match {
                        used_decreases[decrease_idx] = true;
                        
                        let from_address = account_addresses
                            .get(from_index as usize)
                            .map(|s| s.clone())
                            .unwrap_or_else(|| format!("unknown_{}", from_index));
                        
                        let to_address = account_addresses
                            .get(*to_index as usize)
                            .map(|s| s.clone())
                            .unwrap_or_else(|| format!("unknown_{}", to_index));

                        transfers.push(TokenTransfer {
                            signature: signature_str.clone(),
                            from: from_address.clone(),
                            to: to_address.clone(),
                            amount: to_amount,
                            mint: mint.clone(),
                            decimals: *decimals,
                            timestamp,
                        });

                        if SHOW_DEBUG_INFO {
                            debug!("发现复杂代币转账: {} -> {} ({} {} tokens, 比例{:.2})",
                                   &from_address[..8], &to_address[..8], to_amount, &mint[..8], best_ratio);
                        }
                    }
                }
            }
            // 只有增加的情况（可能是mint、空投或者从其他链转入）
            else if !increases.is_empty() && decreases.is_empty() {
                for (to_index, to_change, decimals) in &increases {
                    let to_amount = *to_change as u64;
                    
                    if to_amount > 0 {
                        let to_address = account_addresses
                            .get(*to_index as usize)
                            .map(|s| s.clone())
                            .unwrap_or_else(|| format!("unknown_{}", to_index));

                        if SHOW_DEBUG_INFO {
                            debug!("检测到代币mint/空投/转入: 账户 {} 获得 {} {} tokens",
                                   &to_address[..8], to_amount, &mint[..8]);
                        }
                        
                        // 记录mint操作（可以考虑作为特殊的转账记录）
                        if to_amount >= 1 {  // 过滤掉很小的mint操作
                            transfers.push(TokenTransfer {
                                signature: signature_str.clone(),
                                from: "MINT/AIRDROP".to_string(),
                                to: to_address.clone(),
                                amount: to_amount,
                                mint: mint.clone(),
                                decimals: *decimals,
                                timestamp,
                            });
                        }
                    }
                }
            }
            // 只有减少的情况（可能是burn、转出到其他链或者销毁）
            else if increases.is_empty() && !decreases.is_empty() {
                for (from_index, from_change, decimals) in &decreases {
                    let from_amount = (-from_change) as u64;
                    
                    if from_amount > 0 {
                        let from_address = account_addresses
                            .get(*from_index as usize)
                            .map(|s| s.clone())
                            .unwrap_or_else(|| format!("unknown_{}", from_index));

                        if SHOW_DEBUG_INFO {
                            debug!("检测到代币burn/转出/销毁: 账户 {} 失去 {} {} tokens",
                                   &from_address[..8], from_amount, &mint[..8]);
                        }
                        
                        // 记录burn操作（可以考虑作为特殊的转账记录）
                        if from_amount >= 1 {  // 过滤掉很小的burn操作
                            transfers.push(TokenTransfer {
                                signature: signature_str.clone(),
                                from: from_address.clone(),
                                to: "BURN/DESTROY".to_string(),
                                amount: from_amount,
                                mint: mint.clone(),
                                decimals: *decimals,
                                timestamp,
                            });
                        }
                    }
                }
            }
        }

        Ok(transfers)
    }

    /// 打印转账信息（用于调试）
    pub fn print_transfers(transfers: &[SolTransfer]) {
        if transfers.is_empty() {
            if SHOW_DEBUG_INFO {
                debug!("该交易中未发现SOL转账");
            }
            return;
        }

        println!("🔄 发现 {} 笔SOL转账:", transfers.len());
        for (i, transfer) in transfers.iter().enumerate() {
            let sol_amount = transfer.amount as f64 / 1_000_000_000.0;
            let timestamp = chrono::DateTime::from_timestamp(transfer.timestamp as i64, 0)
                .map(|dt| dt.format("%Y-%m-%d %H:%M:%S").to_string())
                .unwrap_or_else(|| "未知时间".to_string());
            println!(
                "  {}. {} -> {} : {:.9} SOL (时间: {})",
                i + 1,
                &transfer.from[..8],
                &transfer.to[..8],
                sol_amount,
                timestamp
            );
        }
    }

    /// 获取转账总金额（lamports）
    pub fn get_total_transfer_amount(transfers: &[SolTransfer]) -> u64 {
        transfers.iter().map(|t| t.amount).sum()
    }

    /// 检查是否包含大额转账（超过指定阈值，以SOL为单位）
    pub fn has_large_transfer(transfers: &[SolTransfer], threshold_sol: f64) -> bool {
        let threshold_lamports = (threshold_sol * 1_000_000_000.0) as u64;
        transfers.iter().any(|t| t.amount >= threshold_lamports)
    }

    /// 打印代币转账信息
    pub fn print_token_transfers(transfers: &[TokenTransfer]) {
        if transfers.is_empty() {
            if SHOW_DEBUG_INFO {
                debug!("该交易中未发现代币转账");
            }
            return;
        }

        println!("🪙 发现 {} 笔代币转账:", transfers.len());
        for (i, transfer) in transfers.iter().enumerate() {
            let token_amount = transfer.amount as f64 / 10_u64.pow(transfer.decimals) as f64;
            let timestamp = chrono::DateTime::from_timestamp(transfer.timestamp as i64, 0)
                .map(|dt| dt.format("%Y-%m-%d %H:%M:%S").to_string())
                .unwrap_or_else(|| "未知时间".to_string());
            
            // 判断转账类型
            if transfer.from == "MINT/AIRDROP" {
                println!(
                    "  {}. 💰 MINT/空投 -> {} : {:.9} tokens (时间: {})",
                    i + 1,
                    &transfer.to[..8],
                    token_amount,
                    timestamp
                );
            } else if transfer.to == "BURN/DESTROY" {
                println!(
                    "  {}. 🔥 {} -> BURN/销毁 : {:.9} tokens (时间: {})",
                    i + 1,
                    &transfer.from[..8],
                    token_amount,
                    timestamp
                );
            } else {
                println!(
                    "  {}. {} -> {} : {:.9} tokens (时间: {})",
                    i + 1,
                    &transfer.from[..8],
                    &transfer.to[..8],
                    token_amount,
                    timestamp
                );
            }
        }
    }

    /// 获取代币转账总数量
    pub fn get_total_token_transfer_count(transfers: &[TokenTransfer]) -> usize {
        transfers.len()
    }

    /// 按代币mint分组统计转账
    pub fn group_token_transfers_by_mint(transfers: &[TokenTransfer]) -> HashMap<String, Vec<&TokenTransfer>> {
        let mut grouped = HashMap::new();
        for transfer in transfers {
            grouped.entry(transfer.mint.clone())
                .or_insert_with(Vec::new)
                .push(transfer);
        }
        grouped
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_matching_transfer() {
        // 完全匹配
        assert!(TransferParser::is_matching_transfer(1_000_000_000, 1_000_000_000));
        
        // 考虑gas费用的匹配
        assert!(TransferParser::is_matching_transfer(1_005_000, 1_000_000)); // 0.005 SOL gas
        
        // gas费用过高，不匹配
        assert!(!TransferParser::is_matching_transfer(1_020_000_000, 1_000_000_000)); // 0.02 SOL gas
        
        // 接收金额大于发送金额，不匹配
        assert!(!TransferParser::is_matching_transfer(1_000_000, 1_005_000));
    }

    #[test]
    fn test_sol_transfer_debug() {
        let transfer = SolTransfer {
            signature: "test_signature".to_string(),
            from: "from_address".to_string(),
            to: "to_address".to_string(),
            amount: 1_500_000_000, // 1.5 SOL in lamports
            from_index: 0,
            to_index: 1,
            timestamp: 1640995200, // 2022-01-01 00:00:00 UTC
        };

        println!("{:?}", transfer);
        assert_eq!(transfer.amount, 1_500_000_000);
    }
} 