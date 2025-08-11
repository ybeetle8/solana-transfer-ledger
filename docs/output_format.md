# Solana 交易解析器输出格式文档

本文档详细描述了 `transfer_parser.rs` 和 `address_extractor.rs` 模块的输出数据结构和格式。

---

## 目录

1. [SOL 转账解析器 (TransferParser)](#sol-转账解析器-transferparser)
2. [代币转账解析器 (TokenTransfer)](#代币转账解析器-tokentransfer)
3. [地址提取器 (AddressExtractor)](#地址提取器-addressextractor)
4. [输出示例](#输出示例)
5. [数据结构详解](#数据结构详解)

---

## SOL 转账解析器 (TransferParser)

### SOL 转账记录 (SolTransfer)

从交易中解析出的 SOL 转账信息。

```rust
pub struct SolTransfer {
    /// 交易签名
    pub signature: String,
    /// 转出方账户地址
    pub from: String,
    /// 接收方账户地址
    pub to: String,
    /// 转账金额（lamports单位，1 SOL = 10^9 lamports）
    pub amount: u64,
    /// 转出方账户索引
    pub from_index: usize,
    /// 接收方账户索引
    pub to_index: usize,
}
```

### 字段说明

| 字段名 | 类型 | 说明 |
|--------|------|------|
| `signature` | `String` | 交易的唯一标识符，base58 编码的签名 |
| `from` | `String` | 转出方的 Solana 地址 (base58 编码) |
| `to` | `String` | 接收方的 Solana 地址 (base58 编码) |
| `amount` | `u64` | 转账金额，以 lamports 为单位<br/>转换为 SOL: `amount / 1,000,000,000` |
| `from_index` | `usize` | 转出方在交易账户列表中的索引位置 |
| `to_index` | `usize` | 接收方在交易账户列表中的索引位置 |

### 控制台输出格式

```
🔄 发现 X 笔SOL转账:
  1. [from前8位] -> [to前8位] : X.XXXXXXXXX SOL
  2. [from前8位] -> [to前8位] : X.XXXXXXXXX SOL
  ...
```

### 示例输出

```
🔄 发现 1 笔SOL转账:
  1. 9LGCMNhf -> 5CRzXsPB : 5.852616669 SOL
```

---

## 代币转账解析器 (TokenTransfer)

### 代币转账记录 (TokenTransfer)

从交易中解析出的代币转账信息，支持 SPL Token 标准。

```rust
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
    /// 转出方账户索引
    pub from_index: usize,
    /// 接收方账户索引
    pub to_index: usize,
}
```

### 字段说明

| 字段名 | 类型 | 说明 |
|--------|------|------|
| `signature` | `String` | 交易的唯一标识符，base58 编码的签名 |
| `from` | `String` | 转出方的代币账户地址，或特殊值 `"MINT/AIRDROP"` |
| `to` | `String` | 接收方的代币账户地址，或特殊值 `"BURN/DESTROY"` |
| `amount` | `u64` | 转账金额，以代币的最小单位计算 |
| `mint` | `String` | 代币的铸造地址 (mint address) |
| `decimals` | `u32` | 代币的小数位数，用于计算实际金额 |
| `from_index` | `usize` | 转出方在交易账户列表中的索引位置 |
| `to_index` | `usize` | 接收方在交易账户列表中的索引位置 |

### 特殊转账类型

1. **代币铸造/空投**: `from = "MINT/AIRDROP"`
2. **代币销毁**: `to = "BURN/DESTROY"`

### 控制台输出格式

```
🪙 发现 X 笔代币转账:
  1. [类型标志] [from前8位] -> [to前8位] : X.XXXXXXXXX tokens
  2. 💰 MINT/空投 -> [to前8位] : X.XXXXXXXXX tokens
  3. 🔥 [from前8位] -> BURN/销毁 : X.XXXXXXXXX tokens
  ...
```

### 实际金额计算

```
实际代币数量 = amount / (10 ^ decimals)
```

---

## 地址提取器 (AddressExtractor)

### 地址列表

从交易中提取所有相关的 Solana 地址。

```rust
pub fn extract_all_addresses(transaction_update: &SubscribeUpdateTransaction) -> Result<Vec<String>>
```

### 返回值

- **类型**: `Vec<String>`
- **内容**: 所有唯一的 base58 编码的 Solana 地址
- **去重**: 自动去除重复地址

### 提取的地址类型

1. **主账户地址** - 来自 `message.account_keys`
2. **地址表账户** - 来自 `message.address_table_lookups`
3. **加载的可写地址** - 来自 `meta.loaded_writable_addresses`
4. **加载的只读地址** - 来自 `meta.loaded_readonly_addresses`
5. **代币铸造地址** - 来自代币余额记录中的 `mint` 字段
6. **代币所有者地址** - 来自代币余额记录中的 `owner` 字段
7. **奖励接收者地址** - 来自 `meta.rewards`
8. **返回数据程序地址** - 来自 `meta.return_data.program_id`

### 控制台输出格式

```
🔍 交易地址列表 (X 个):
   1. [完整的base58地址]
   2. [完整的base58地址]
   ...
```

### 示例输出

```
🔍 交易地址列表 (46 个):
   1. 11111111111111111111111111111111
   2. 9LGCMNhfRrk18gBdaBhakU11KDNZKZHHhM7QqZ2xLsH7
   3. 5CRzXsPBuE8N26JuScS7VFoA2dQDSLvhAJ9cxYfkP29d
   ...
```

---

## 输出示例

### 完整的交易解析输出

```
📝 签名: 61hANGzb47nALJZST7Q2kgwE5FUpbqKjdWMfRzRHqmKZvMdXj4sf7tTWibgti7b2c68SDg6GERRtdavzHh9VmBJi

🔄 发现 1 笔SOL转账:
  1. 9LGCMNhf -> 5CRzXsPB : 5.852616669 SOL

🪙 发现 2 笔代币转账:
  1. ATokenGPv -> ComputeBu : 1000.000000 tokens
  2. 💰 MINT/空投 -> 5CRzXsPB : 50.000000 tokens

🔍 交易地址列表 (46 个):
   1. 11111111111111111111111111111111
   2. 9LGCMNhfRrk18gBdaBhakU11KDNZKZHHhM7QqZ2xLsH7
   3. 5CRzXsPBuE8N26JuScS7VFoA2dQDSLvhAJ9cxYfkP29d
   4. TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA
   5. ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL
   ...

```

---

## 数据结构详解

### 1. 时间单位说明

- **Lamports**: SOL 的最小单位，1 SOL = 1,000,000,000 lamports
- **代币最小单位**: 取决于代币的 `decimals` 参数

### 2. 地址格式

- 所有地址均为 base58 编码的字符串
- 标准 Solana 地址长度为 32 字节，编码后约 44 个字符
- 系统程序地址: `11111111111111111111111111111111`

### 3. 索引说明

- `from_index` 和 `to_index` 指向交易中的账户列表位置
- 索引从 0 开始
- 用于追溯账户在原始交易数据中的位置

### 4. 特殊值处理

- 空字符串和系统地址 (`11111...`) 会被过滤
- 代币铸造/销毁操作使用特殊的 `from`/`to` 值标识

### 5. 错误处理

- 解析失败时返回空结果，不会中断程序运行
- 所有错误都会记录到日志中
- 数据不完整时会跳过解析

---

## 使用示例

### 在代码中使用解析器

```rust
use crate::transfer_parser::TransferParser;
use crate::address_extractor::AddressExtractor;

// 解析 SOL 转账
let sol_transfers = TransferParser::parse_sol_transfers(&transaction_update)?;
for transfer in &sol_transfers {
    println!("SOL转账: {} -> {}, 金额: {:.9} SOL", 
        transfer.from, transfer.to, 
        transfer.amount as f64 / 1_000_000_000.0);
}

// 解析代币转账
let token_transfers = TransferParser::parse_token_transfers(&transaction_update)?;
for transfer in &token_transfers {
    let amount = transfer.amount as f64 / 10_u64.pow(transfer.decimals) as f64;
    println!("代币转账: {} -> {}, 金额: {:.9} tokens, 代币: {}", 
        transfer.from, transfer.to, amount, transfer.mint);
}

// 提取所有地址
let addresses = AddressExtractor::extract_all_addresses(&transaction_update)?;
println!("涉及的地址数量: {}", addresses.len());
for (i, address) in addresses.iter().enumerate() {
    println!("  {}. {}", i + 1, address);
}
```

---

## 版本信息

- **文档版本**: 1.0
- **最后更新**: 2024年
- **兼容性**: Solana v1.14+ / yellowstone-grpc-proto v1.x 