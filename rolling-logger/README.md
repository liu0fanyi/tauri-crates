# Rolling Logger Crate

一个为Tauri应用设计的滚动文件日志库，支持循环缓冲区。

## 特性

- **10MB大小限制**：日志文件最大10MB
- **循环覆盖**：超过限制后新日志覆盖旧日志
- **多级别日志**：支持INFO、DEBUG、WARN、ERROR级别
- **时间戳**：每条日志包含精确时间戳
- **线程安全**：使用Arc+Mutex保证线程安全
- **全局访问**：提供简单易用的全局API

## 使用方式

### 基本使用

```rust
use rolling_logger;

// 初始化日志系统
rolling_logger::init_logger(std::path::PathBuf::from("/path/to/logs"))?;

// 记录日志
rolling_logger::info("Application started");
rolling_logger::warn("This is a warning");
rolling_logger::error("Something went wrong");
rolling_logger::debug("Debug information");
```

### 高级使用

```rust
use rolling_logger::RollingLogger;

// 创建独立的logger实例
let logger = RollingLogger::new("/path/to/logs")?;

// 直接使用logger实例
logger.info("Direct log message")?;

// 读取日志内容
let logs = logger.read_logs()?;
println!("Current log size: {} bytes", logger.current_size());
```

## 配置

- **最大日志大小**：10MB（常量 `MAX_LOG_SIZE`）
- **日志文件名**：`app.log`
- **时间戳格式**：`%Y-%m-%d %H:%M:%S%.3f`

## 测试

```bash
cargo test -p rolling-logger
```

## 许可证

MIT