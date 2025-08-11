# 自增ID分配器设计文档

## 概述

本文档描述了一个高性能、线程安全的自增ID分配器实现方案，专门用于异步环境中生成唯一ID，以替代64字符的交易签名，大幅减少存储空间占用。

---

## 设计目标

### 核心要求
1. **唯一性**：保证生成的ID全局唯一，永不重复
2. **线程安全**：支持多线程/异步环境并发访问
3. **崩溃安全**：程序重启后能正确恢复，不会产生重复ID
4. **高性能**：减少磁盘IO，优化内存访问
5. **空间节省**：用u32(4字节)替代签名(64字节)，节省93.75%空间

### 性能指标
- 单线程性能：>1,000,000 ID/秒
- 多线程性能：>500,000 ID/秒 (8线程)
- 磁盘写入频率：每10,000个ID写入1次
- 重启恢复时间：<100ms

---

## 核心设计原理

### 批量预分配策略
```
内存中维护一个ID范围 [current_id, max_id)
当范围用尽时，从DB预分配下一批：
1. 从DB读取 stored_next_id
2. 计算新范围：[stored_next_id, stored_next_id + batch_size)  
3. 更新DB：stored_next_id += batch_size
4. 更新内存范围
```

### 崩溃安全机制
```
正常运行：内存范围 [100000, 110000)，DB中存储 110000
程序崩溃：内存中的 [current_id, 110000) 范围丢失
程序重启：从DB读取 110000，分配新范围 [110000, 120000)
结果：ID连续性有间隙，但保证不重复
```

---

## 数据结构设计

### 主要组件

```rust
/// ID分配器配置
#[derive(Debug, Clone)]
pub struct IdAllocatorConfig {
    /// 批次大小（一次预分配的ID数量）
    pub batch_size: u32,
    /// 最小批次大小
    pub min_batch_size: u32,
    /// 最大批次大小  
    pub max_batch_size: u32,
    /// 预取阈值（剩余多少个ID时开始预取下一批）
    pub prefetch_threshold: u32,
    /// 是否启用动态批次调整
    pub enable_dynamic_batch: bool,
}

/// ID分配器主结构
pub struct IdAllocator {
    /// 当前可用的ID值
    current_id: AtomicU32,
    /// 当前批次的最大ID值（不包含）
    max_id: AtomicU32,
    /// 配置参数
    config: IdAllocatorConfig,
    /// RocksDB连接
    db: Arc<DB>,
    /// 用于批次分配的互斥锁
    allocation_mutex: Arc<Mutex<()>>,
    /// 统计信息
    stats: Arc<IdStats>,
    /// 预取状态标记
    prefetch_in_progress: AtomicBool,
}

/// 统计信息
#[derive(Debug, Default)]
pub struct IdStats {
    /// 总分配的ID数量
    pub total_allocated: AtomicU64,
    /// 总使用的ID数量
    pub total_used: AtomicU64,
    /// 批次分配次数
    pub batch_allocations: AtomicU64,
    /// 数据库写入次数
    pub db_writes: AtomicU64,
    /// 分配耗时统计（微秒）
    pub allocation_time_us: AtomicU64,
}
```

---

## 线程安全实现

### 1. 原子操作策略

```rust
impl IdAllocator {
    /// 获取下一个ID（线程安全）
    pub fn next_id(&self) -> Result<u32, IdAllocatorError> {
        loop {
            let current = self.current_id.load(Ordering::Relaxed);
            let max = self.max_id.load(Ordering::Acquire);
            
            // 检查是否需要分配新批次
            if current >= max {
                self.allocate_new_batch()?;
                continue;
            }
            
            // 原子性递增
            match self.current_id.compare_exchange_weak(
                current,
                current + 1,
                Ordering::Relaxed,
                Ordering::Relaxed
            ) {
                Ok(_) => {
                    self.stats.total_used.fetch_add(1, Ordering::Relaxed);
                    
                    // 检查是否需要异步预取
                    if max - current <= self.config.prefetch_threshold {
                        self.async_prefetch();
                    }
                    
                    return Ok(current + 1);
                }
                Err(_) => continue, // CAS失败，重试
            }
        }
    }
    
    /// 分配新批次（加锁保护）
    fn allocate_new_batch(&self) -> Result<(), IdAllocatorError> {
        let _lock = self.allocation_mutex.lock().unwrap();
        
        // Double-check locking 模式
        let current = self.current_id.load(Ordering::Relaxed);
        let max = self.max_id.load(Ordering::Acquire);
        
        if current < max {
            return Ok(); // 其他线程已经分配了
        }
        
        let start_time = std::time::Instant::now();
        
        // 从数据库获取下一个ID段
        let stored_id = self.get_stored_next_id()?;
        let new_max = stored_id + self.config.batch_size;
        
        // 更新数据库
        self.update_stored_next_id(new_max)?;
        
        // 更新内存状态
        self.current_id.store(stored_id, Ordering::Relaxed);
        self.max_id.store(new_max, Ordering::Release);
        
        // 更新统计
        self.stats.total_allocated.fetch_add(self.config.batch_size as u64, Ordering::Relaxed);
        self.stats.batch_allocations.fetch_add(1, Ordering::Relaxed);
        self.stats.db_writes.fetch_add(1, Ordering::Relaxed);
        self.stats.allocation_time_us.fetch_add(
            start_time.elapsed().as_micros() as u64, 
            Ordering::Relaxed
        );
        
        Ok(())
    }
}
```

### 2. 异步预取机制

```rust
impl IdAllocator {
    /// 异步预取下一批ID
    fn async_prefetch(&self) {
        // 使用原子标记避免重复预取
        if self.prefetch_in_progress.compare_exchange(
            false, 
            true, 
            Ordering::Acquire, 
            Ordering::Relaxed
        ).is_err() {
            return; // 已经有预取在进行中
        }
        
        let allocator = self.clone();
        tokio::spawn(async move {
            let result = allocator.prefetch_next_batch().await;
            allocator.prefetch_in_progress.store(false, Ordering::Release);
            
            if let Err(e) = result {
                log::warn!("异步预取失败: {}", e);
            }
        });
    }
    
    /// 预取下一批次
    async fn prefetch_next_batch(&self) -> Result<(), IdAllocatorError> {
        let _lock = self.allocation_mutex.lock().unwrap();
        
        let current = self.current_id.load(Ordering::Relaxed);
        let max = self.max_id.load(Ordering::Acquire);
        
        // 如果剩余ID足够，无需预取
        if max - current > self.config.prefetch_threshold {
            return Ok(());
        }
        
        // 异步获取和更新数据库
        let stored_id = self.async_get_stored_next_id().await?;
        let new_batch_size = self.calculate_dynamic_batch_size();
        let new_max = stored_id + new_batch_size;
        
        self.async_update_stored_next_id(new_max).await?;
        
        // 预分配完成，等待当前批次用完时切换
        Ok(())
    }
}
```

---

## 存储层设计

### RocksDB Key-Value 结构

```rust
// 存储键值对
const NEXT_ID_KEY: &[u8] = b"id_allocator:next_id";
const CONFIG_KEY: &[u8] = b"id_allocator:config";
const STATS_KEY: &[u8] = b"id_allocator:stats";

/// 存储操作
impl IdAllocator {
    /// 获取存储的下一个ID
    fn get_stored_next_id(&self) -> Result<u32, IdAllocatorError> {
        match self.db.get(NEXT_ID_KEY)? {
            Some(bytes) => {
                if bytes.len() == 4 {
                    Ok(u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
                } else {
                    Err(IdAllocatorError::CorruptedData)
                }
            }
            None => {
                // 首次启动，初始化为1
                self.update_stored_next_id(1)?;
                Ok(1)
            }
        }
    }
    
    /// 更新存储的下一个ID  
    fn update_stored_next_id(&self, next_id: u32) -> Result<(), IdAllocatorError> {
        let bytes = next_id.to_be_bytes();
        self.db.put(NEXT_ID_KEY, &bytes)?;
        
        // 可选：强制同步到磁盘
        if self.config.enable_sync {
            self.db.sync_wal()?;
        }
        
        Ok(())
    }
    
    /// 异步版本的数据库操作
    async fn async_get_stored_next_id(&self) -> Result<u32, IdAllocatorError> {
        let db = self.db.clone();
        tokio::task::spawn_blocking(move || {
            // 在线程池中执行阻塞的数据库操作
            match db.get(NEXT_ID_KEY)? {
                Some(bytes) => Ok(u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]])),
                None => Ok(1),
            }
        }).await.map_err(|e| IdAllocatorError::AsyncError(e.to_string()))?
    }
    
    async fn async_update_stored_next_id(&self, next_id: u32) -> Result<(), IdAllocatorError> {
        let db = self.db.clone();
        tokio::task::spawn_blocking(move || {
            let bytes = next_id.to_be_bytes();
            db.put(NEXT_ID_KEY, &bytes)
        }).await.map_err(|e| IdAllocatorError::AsyncError(e.to_string()))?
    }
}
```

---

## 动态批次调整

### 自适应批次大小算法

```rust
impl IdAllocator {
    /// 计算动态批次大小
    fn calculate_dynamic_batch_size(&self) -> u32 {
        if !self.config.enable_dynamic_batch {
            return self.config.batch_size;
        }
        
        let stats = &self.stats;
        let used = stats.total_used.load(Ordering::Relaxed);
        let allocated = stats.total_allocated.load(Ordering::Relaxed);
        
        if allocated == 0 {
            return self.config.batch_size;
        }
        
        // 计算利用率
        let utilization = used as f64 / allocated as f64;
        let current_batch = self.config.batch_size;
        
        let new_batch = match utilization {
            // 利用率很高(>95%)，增大批次
            r if r > 0.95 => {
                (current_batch * 2).min(self.config.max_batch_size)
            }
            // 利用率很低(<50%)，减小批次
            r if r < 0.50 => {
                (current_batch / 2).max(self.config.min_batch_size)
            }
            // 利用率正常，保持不变
            _ => current_batch,
        };
        
        log::debug!("动态调整批次大小: {} -> {} (利用率: {:.2}%)", 
                   current_batch, new_batch, utilization * 100.0);
        
        new_batch
    }
    
    /// 获取使用频率统计
    fn get_usage_rate(&self) -> f64 {
        let allocations = self.stats.batch_allocations.load(Ordering::Relaxed);
        let total_time = self.stats.allocation_time_us.load(Ordering::Relaxed);
        
        if total_time == 0 { return 0.0; }
        
        // IDs per second
        let used = self.stats.total_used.load(Ordering::Relaxed);
        let time_seconds = total_time as f64 / 1_000_000.0;
        
        used as f64 / time_seconds
    }
}
```

---

## 错误处理

### 错误类型定义

```rust
#[derive(Debug, thiserror::Error)]
pub enum IdAllocatorError {
    #[error("RocksDB 错误: {0}")]
    DatabaseError(#[from] rocksdb::Error),
    
    #[error("数据损坏: 存储的ID格式不正确")]
    CorruptedData,
    
    #[error("ID耗尽: 已达到u32最大值")]
    IdExhausted,
    
    #[error("异步操作错误: {0}")]
    AsyncError(String),
    
    #[error("配置错误: {0}")]
    ConfigError(String),
    
    #[error("锁竞争超时")]
    LockTimeout,
}
```

### 错误恢复策略

```rust
impl IdAllocator {
    /// 错误恢复和重试机制
    pub fn next_id_with_retry(&self, max_retries: u32) -> Result<u32, IdAllocatorError> {
        let mut retries = 0;
        
        loop {
            match self.next_id() {
                Ok(id) => return Ok(id),
                Err(IdAllocatorError::DatabaseError(_)) if retries < max_retries => {
                    retries += 1;
                    log::warn!("数据库错误，第{}次重试", retries);
                    std::thread::sleep(std::time::Duration::from_millis(10 * retries as u64));
                    continue;
                }
                Err(e) => return Err(e),
            }
        }
    }
    
    /// 健康检查
    pub fn health_check(&self) -> Result<HealthStatus, IdAllocatorError> {
        let current = self.current_id.load(Ordering::Relaxed);
        let max = self.max_id.load(Ordering::Relaxed);
        let remaining = max.saturating_sub(current);
        
        // 检查ID是否即将耗尽
        if current > u32::MAX - 1000000 {
            return Ok(HealthStatus::Critical("ID即将耗尽".to_string()));
        }
        
        // 检查剩余ID是否过少
        if remaining < self.config.prefetch_threshold {
            return Ok(HealthStatus::Warning("剩余ID较少".to_string()));
        }
        
        Ok(HealthStatus::Healthy)
    }
}

#[derive(Debug)]
pub enum HealthStatus {
    Healthy,
    Warning(String),
    Critical(String),
}
```

---

## 使用示例

### 基本用法

```rust
use tokio;
use std::sync::Arc;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 初始化数据库
    let db = Arc::new(DB::open_default("./id_allocator_db")?);
    
    // 创建配置
    let config = IdAllocatorConfig {
        batch_size: 10000,
        min_batch_size: 1000,
        max_batch_size: 100000,
        prefetch_threshold: 1000,
        enable_dynamic_batch: true,
    };
    
    // 创建ID分配器
    let allocator = Arc::new(IdAllocator::new(db, config)?);
    
    // 在多个异步任务中使用
    let mut handles = vec![];
    
    for i in 0..8 {
        let allocator_clone = allocator.clone();
        let handle = tokio::spawn(async move {
            for _ in 0..10000 {
                match allocator_clone.next_id() {
                    Ok(id) => {
                        // 使用ID处理业务逻辑
                        process_transaction(id).await;
                    }
                    Err(e) => {
                        eprintln!("线程{} ID分配失败: {}", i, e);
                        break;
                    }
                }
            }
        });
        handles.push(handle);
    }
    
    // 等待所有任务完成
    for handle in handles {
        handle.await?;
    }
    
    // 打印统计信息
    allocator.print_stats();
    
    Ok(())
}

async fn process_transaction(id: u32) {
    // 模拟业务处理
    tokio::time::sleep(tokio::time::Duration::from_micros(10)).await;
    println!("处理交易 ID: {}", id);
}
```

### 集成到现有系统

```rust
// 在交易解析器中集成ID分配器
pub struct TransactionProcessor {
    id_allocator: Arc<IdAllocator>,
    db: Arc<DB>,
}

impl TransactionProcessor {
    pub async fn process_transaction(
        &self,
        signature: &str,
        transaction_data: &TransactionData,
    ) -> Result<u32, ProcessError> {
        // 分配唯一ID
        let transaction_id = self.id_allocator.next_id()?;
        
        // 存储签名到ID的映射
        let sig_to_id_key = format!("sig2id:{}", signature);
        let id_to_sig_key = format!("id2sig:{}", transaction_id);
        
        // 批量写入
        let mut batch = WriteBatch::default();
        batch.put(&sig_to_id_key, &transaction_id.to_be_bytes());
        batch.put(&id_to_sig_key, signature.as_bytes());
        
        self.db.write(batch)?;
        
        log::info!("交易 {} 分配ID: {}", signature, transaction_id);
        Ok(transaction_id)
    }
}
```

---

## 监控和调试

### 性能监控

```rust
impl IdAllocator {
    /// 打印详细统计信息
    pub fn print_stats(&self) {
        let stats = &self.stats;
        let used = stats.total_used.load(Ordering::Relaxed);
        let allocated = stats.total_allocated.load(Ordering::Relaxed);
        let batch_count = stats.batch_allocations.load(Ordering::Relaxed);
        let db_writes = stats.db_writes.load(Ordering::Relaxed);
        let avg_time = if batch_count > 0 {
            stats.allocation_time_us.load(Ordering::Relaxed) / batch_count
        } else { 0 };
        
        println!("=== ID分配器统计信息 ===");
        println!("已使用ID数量: {}", used);
        println!("已分配ID数量: {}", allocated);
        println!("利用率: {:.2}%", used as f64 / allocated as f64 * 100.0);
        println!("批次分配次数: {}", batch_count);
        println!("数据库写入次数: {}", db_writes);
        println!("平均分配耗时: {} μs", avg_time);
        println!("当前批次范围: [{}, {})", 
                self.current_id.load(Ordering::Relaxed),
                self.max_id.load(Ordering::Relaxed));
    }
    
    /// 获取JSON格式的统计信息
    pub fn get_stats_json(&self) -> serde_json::Value {
        serde_json::json!({
            "total_used": self.stats.total_used.load(Ordering::Relaxed),
            "total_allocated": self.stats.total_allocated.load(Ordering::Relaxed),
            "batch_allocations": self.stats.batch_allocations.load(Ordering::Relaxed),
            "db_writes": self.stats.db_writes.load(Ordering::Relaxed),
            "current_id": self.current_id.load(Ordering::Relaxed),
            "max_id": self.max_id.load(Ordering::Relaxed),
            "utilization_percent": {
                let used = self.stats.total_used.load(Ordering::Relaxed) as f64;
                let allocated = self.stats.total_allocated.load(Ordering::Relaxed) as f64;
                if allocated > 0.0 { used / allocated * 100.0 } else { 0.0 }
            }
        })
    }
}
```

---

## 性能基准测试

### 测试场景

```rust
#[cfg(test)]
mod benchmarks {
    use super::*;
    use criterion::{black_box, criterion_group, criterion_main, Criterion};
    
    fn benchmark_single_thread(c: &mut Criterion) {
        let db = Arc::new(DB::open_default("./bench_db").unwrap());
        let config = IdAllocatorConfig::default();
        let allocator = IdAllocator::new(db, config).unwrap();
        
        c.bench_function("single_thread_id_allocation", |b| {
            b.iter(|| {
                black_box(allocator.next_id().unwrap());
            })
        });
    }
    
    fn benchmark_multi_thread(c: &mut Criterion) {
        let db = Arc::new(DB::open_default("./bench_db_mt").unwrap());
        let config = IdAllocatorConfig::default();
        let allocator = Arc::new(IdAllocator::new(db, config).unwrap());
        
        c.bench_function("multi_thread_id_allocation", |b| {
            b.iter(|| {
                let mut handles = vec![];
                for _ in 0..8 {
                    let allocator_clone = allocator.clone();
                    let handle = std::thread::spawn(move || {
                        for _ in 0..1000 {
                            black_box(allocator_clone.next_id().unwrap());
                        }
                    });
                    handles.push(handle);
                }
                for handle in handles {
                    handle.join().unwrap();
                }
            })
        });
    }
    
    criterion_group!(benches, benchmark_single_thread, benchmark_multi_thread);
    criterion_main!(benches);
}
```

### 预期性能指标

| 场景 | 性能指标 | 内存使用 | 磁盘IO |
|------|---------|---------|--------|
| 单线程顺序分配 | >1,000,000 ID/秒 | ~1MB | 1次/10,000 ID |
| 8线程并发分配 | >500,000 ID/秒 | ~8MB | 8次/10,000 ID |
| 异步批量分配 | >800,000 ID/秒 | ~2MB | 1次/10,000 ID |

---

## 部署建议

### 1. 数据库配置
```rust
// RocksDB 优化配置
let mut opts = Options::default();
opts.create_if_missing(true);
opts.set_max_open_files(1000);
opts.set_use_fsync(false);  // 提高写入性能
opts.set_bytes_per_sync(8 << 20); // 8MB
opts.set_write_buffer_size(64 << 20); // 64MB
opts.set_max_write_buffer_number(3);
opts.set_target_file_size_base(64 << 20); // 64MB
```

### 2. 系统资源
- **CPU**: 2核心 (支持8个并发线程)
- **内存**: 512MB (包含RocksDB缓存)
- **磁盘**: SSD推荐 (提高数据库写入性能)
- **网络**: 无特殊要求

### 3. 监控指标
- ID分配速率 (IDs/second)
- 数据库写入延迟
- 批次利用率
- 内存使用量
- 磁盘空间增长

---

## 版本历史

- **v1.0**: 基本的批量预分配实现
- **v1.1**: 添加异步预取机制
- **v1.2**: 动态批次大小调整
- **v1.3**: 完善错误处理和监控

---

## 总结

本ID分配器设计方案提供了一个高性能、线程安全、崩溃安全的解决方案，通过批量预分配策略大幅减少磁盘IO，通过原子操作保证线程安全，通过异步预取提高并发性能。适用于高频的ID生成场景，特别是区块链交易处理系统。 