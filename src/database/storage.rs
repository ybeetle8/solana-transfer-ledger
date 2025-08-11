use anyhow::{Result, Context};
use rocksdb::{DB, Options, Direction, IteratorMode};
use serde::{Serialize, Deserialize, de::DeserializeOwned};
use std::path::Path;
use std::sync::Arc;
use tracing::{info, debug};

/// RocksDB 存储管理器
#[derive(Clone)]
#[derive(Debug)]
pub struct StorageManager {
    db: Arc<DB>,
    key_prefix_length: usize,
}

/// 键值对结构
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyValue<T> {
    pub key: String,
    pub value: T,
}

/// 存储操作结果
#[derive(Debug)]
pub struct StorageResult {
    pub success: bool,
    pub message: String,
}

impl StorageManager {
    /// 创建新的存储管理器实例
    pub fn new<P: AsRef<Path>>(db_path: P, key_prefix_length: usize) -> Result<Self> {
        // 创建数据库目录
        let path = db_path.as_ref();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).context("创建数据库目录失败")?;
        }

        // 配置 RocksDB 选项
        let mut opts = Options::default();
        opts.create_if_missing(true);
        opts.set_max_open_files(1000);
        opts.set_use_fsync(false);
        opts.set_bytes_per_sync(8388608);
        opts.optimize_for_point_lookup(1024);
        opts.set_table_cache_num_shard_bits(6);
        opts.set_max_write_buffer_number(32);
        opts.set_write_buffer_size(536870912);
        opts.set_target_file_size_base(1073741824);
        opts.set_min_write_buffer_number_to_merge(4);
        opts.set_level_zero_stop_writes_trigger(2000);
        opts.set_level_zero_slowdown_writes_trigger(0);
        opts.set_compaction_style(rocksdb::DBCompactionStyle::Universal);

        // 打开数据库
        let db = DB::open(&opts, path).context("打开 RocksDB 数据库失败")?;
        
        info!("RocksDB 数据库已成功打开: {:?}", path);
        
        Ok(StorageManager {
            db: Arc::new(db),
            key_prefix_length,
        })
    }

    /// 生成带前缀的键
    pub fn make_key(&self, prefix: &str, key: &str) -> Result<String> {
        if prefix.len() != self.key_prefix_length {
            return Err(anyhow::anyhow!(
                "键前缀长度必须为 {} 位，实际为 {} 位", 
                self.key_prefix_length, 
                prefix.len()
            ));
        }
        Ok(format!("{}{}", prefix, key))
    }

    /// 验证键前缀
    pub fn validate_key_prefix<'a>(&self, key: &'a str) -> Result<(&'a str, &'a str)> {
        if key.len() < self.key_prefix_length {
            return Err(anyhow::anyhow!(
                "键长度不足，至少需要 {} 位前缀", 
                self.key_prefix_length
            ));
        }
        
        let (prefix, suffix) = key.split_at(self.key_prefix_length);
        Ok((prefix, suffix))
    }

    /// 存储键值对（通用方法）
    pub fn put<T: Serialize>(&self, key: &str, value: &T) -> Result<StorageResult> {
        // 序列化值
        let serialized_value = serde_json::to_vec(value)
            .context("序列化值失败")?;

        // 存储到数据库
        self.db.put(key.as_bytes(), serialized_value)
            .context("存储数据到 RocksDB 失败")?;

        debug!("成功存储数据: key={}", key);
        
        Ok(StorageResult {
            success: true,
            message: format!("成功存储键: {}", key),
        })
    }

    /// 获取值（通用方法）
    pub fn get<T: DeserializeOwned>(&self, key: &str) -> Result<Option<T>> {
        match self.db.get(key.as_bytes()).context("从 RocksDB 读取数据失败")? {
            Some(data) => {
                let value: T = serde_json::from_slice(&data)
                    .context("反序列化数据失败")?;
                debug!("成功读取数据: key={}", key);
                Ok(Some(value))
            }
            None => {
                debug!("未找到数据: key={}", key);
                Ok(None)
            }
        }
    }

    /// 删除键值对
    pub fn delete(&self, key: &str) -> Result<StorageResult> {
        self.db.delete(key.as_bytes())
            .context("从 RocksDB 删除数据失败")?;

        debug!("成功删除数据: key={}", key);
        
        Ok(StorageResult {
            success: true,
            message: format!("成功删除键: {}", key),
        })
    }

    /// 检查键是否存在
    pub fn exists(&self, key: &str) -> Result<bool> {
        match self.db.get(key.as_bytes()).context("检查键是否存在失败")? {
            Some(_) => Ok(true),
            None => Ok(false),
        }
    }

    /// 按前缀获取所有键值对
    pub fn get_by_prefix<T: DeserializeOwned>(&self, prefix: &str) -> Result<Vec<KeyValue<T>>> {
        let mut results = Vec::new();
        let prefix_bytes = prefix.as_bytes();

        let iter = self.db.iterator(IteratorMode::From(prefix_bytes, Direction::Forward));
        
        for item in iter {
            let (key_bytes, value_bytes) = item.context("迭代数据库失败")?;
            let key_str = String::from_utf8(key_bytes.to_vec())
                .context("键不是有效的 UTF-8 字符串")?;

            // 检查是否仍然匹配前缀
            if !key_str.starts_with(prefix) {
                break;
            }

            let value: T = serde_json::from_slice(&value_bytes)
                .context("反序列化数据失败")?;

            results.push(KeyValue {
                key: key_str,
                value,
            });
        }

        debug!("按前缀查询到 {} 条记录: prefix={}", results.len(), prefix);
        Ok(results)
    }

    /// 获取所有键（按前缀过滤）
    pub fn get_keys_by_prefix(&self, prefix: &str) -> Result<Vec<String>> {
        let mut keys = Vec::new();
        let prefix_bytes = prefix.as_bytes();

        let iter = self.db.iterator(IteratorMode::From(prefix_bytes, Direction::Forward));
        
        for item in iter {
            let (key_bytes, _) = item.context("迭代数据库失败")?;
            let key_str = String::from_utf8(key_bytes.to_vec())
                .context("键不是有效的 UTF-8 字符串")?;

            // 检查是否仍然匹配前缀
            if !key_str.starts_with(prefix) {
                break;
            }

            keys.push(key_str);
        }

        debug!("查询到 {} 个键: prefix={}", keys.len(), prefix);
        Ok(keys)
    }

    /// 批量存储
    pub fn batch_put<T: Serialize>(&self, items: Vec<(String, T)>) -> Result<StorageResult> {
        let mut batch = rocksdb::WriteBatch::default();
        
        for (key, value) in items.iter() {
            let serialized_value = serde_json::to_vec(value)
                .context("序列化值失败")?;
            batch.put(key.as_bytes(), serialized_value);
        }

        self.db.write(batch).context("批量写入 RocksDB 失败")?;

        let message = format!("成功批量存储 {} 条记录", items.len());
        info!("{}", message);
        
        Ok(StorageResult {
            success: true,
            message,
        })
    }

    /// 获取数据库统计信息
    pub fn get_stats(&self) -> Result<String> {
        let stats = self.db.property_value("rocksdb.stats")
            .context("获取数据库统计信息失败")?
            .unwrap_or_else(|| "无统计信息".to_string());
        Ok(stats)
    }

    /// 获取压缩相关统计信息
    pub fn get_compaction_stats(&self) -> Result<String> {
        let mut stats_info = String::new();
        
        // 获取各种压缩相关统计
        if let Ok(Some(compaction_pending)) = self.db.property_value("rocksdb.compaction-pending") {
            stats_info.push_str(&format!("压缩等待中: {}\n", compaction_pending));
        }
        
        if let Ok(Some(num_running_compactions)) = self.db.property_value("rocksdb.num-running-compactions") {
            stats_info.push_str(&format!("运行中的压缩: {}\n", num_running_compactions));
        }
        
        if let Ok(Some(level0_files)) = self.db.property_value("rocksdb.num-files-at-level0") {
            stats_info.push_str(&format!("Level 0 文件数: {}\n", level0_files));
        }
        
        if let Ok(Some(total_sst_files)) = self.db.property_value("rocksdb.total-sst-files-size") {
            stats_info.push_str(&format!("SST 文件总大小: {} bytes\n", total_sst_files));
        }
        
        if let Ok(Some(live_sst_files)) = self.db.property_value("rocksdb.live-sst-files-size") {
            stats_info.push_str(&format!("活跃 SST 文件大小: {} bytes\n", live_sst_files));
        }
        
        Ok(stats_info)
    }

    /// 压缩数据库
    pub fn compact(&self) -> Result<StorageResult> {
        self.db.compact_range(Option::<&[u8]>::None, Option::<&[u8]>::None);
        
        let message = "数据库压缩完成".to_string();
        info!("{}", message);
        
        Ok(StorageResult {
            success: true,
            message,
        })
    }
}

impl Drop for StorageManager {
    fn drop(&mut self) {
        info!("RocksDB 存储管理器正在关闭");
    }
} 