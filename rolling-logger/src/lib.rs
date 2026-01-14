use std::fs::{File, OpenOptions};
use std::io::{self, Write, Seek, SeekFrom, Read};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use chrono::Local;
use tracing::{
    field::{Field, Visit},
    Event, Subscriber,
};
use tracing_subscriber::{
    layer::{Context, Layer},
    prelude::*,
    registry::LookupSpan,
};

const MAX_LOG_SIZE: u64 = 10 * 1024 * 1024; // 10MB

/// Rolling file appender that implements circular buffer
pub struct RollingFileAppender {
    file: Arc<Mutex<File>>,
    current_position: Arc<Mutex<u64>>,
    file_size: Arc<Mutex<u64>>,
}

impl RollingFileAppender {
    pub fn new<P: AsRef<Path>>(log_dir: P) -> io::Result<Self> {
        let log_dir = log_dir.as_ref();
        std::fs::create_dir_all(log_dir)?;
        
        let file_path = log_dir.join("app.log");
        
        // Open or create the log file
        let mut file = OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .open(&file_path)?;
            
        // Get current file size
        let file_size = file.metadata()?.len();
        let current_position = if file_size >= MAX_LOG_SIZE {
            // If file is already at max size, start from beginning (circular)
            0
        } else {
            file_size
        };
        
        // Seek to the appropriate position
        file.seek(SeekFrom::Start(current_position))?;
        
        Ok(RollingFileAppender {
            file: Arc::new(Mutex::new(file)),
            current_position: Arc::new(Mutex::new(current_position)),
            file_size: Arc::new(Mutex::new(file_size)),
        })
    }
    
    /// Write a formatted log entry to the file
    pub fn write(&self, formatted_entry: &str) -> io::Result<()> {
        let entry_bytes = formatted_entry.as_bytes();
        let entry_size = entry_bytes.len() as u64;
        
        let mut file = self.file.lock().unwrap();
        let mut current_pos = self.current_position.lock().unwrap();
        let mut file_size = self.file_size.lock().unwrap();
        
        // Check if we need to wrap around
        if *current_pos + entry_size > MAX_LOG_SIZE {
            // Wrap around to the beginning
            *current_pos = 0;
            file.seek(SeekFrom::Start(0))?;
        }
        
        // Write the log entry
        file.write_all(entry_bytes)?;
        file.flush()?;
        
        // Update position and size
        *current_pos += entry_size;
        if *current_pos > *file_size {
            *file_size = *current_pos;
        }
        
        Ok(())
    }
    
    /// Read the entire log file content
    pub fn read_logs(&self) -> io::Result<String> {
        let mut file = self.file.lock().unwrap();
        let mut content = String::new();
        file.seek(SeekFrom::Start(0))?;
        file.read_to_string(&mut content)?;
        Ok(content)
    }
    
    /// Get current log file size
    pub fn current_size(&self) -> u64 {
        *self.file_size.lock().unwrap()
    }
    
    /// Get current write position
    pub fn current_position(&self) -> u64 {
        *self.current_position.lock().unwrap()
    }
}

/// Tracing Layer that writes to rolling file
pub struct RollingFileLayer {
    appender: Arc<RollingFileAppender>,
}

impl RollingFileLayer {
    pub fn new<P: AsRef<Path>>(log_dir: P) -> io::Result<Self> {
        Ok(RollingFileLayer {
            appender: Arc::new(RollingFileAppender::new(log_dir)?),
        })
    }
    
    /// Format a log event for writing to file
    fn format_event<S>(&self, event: &Event, ctx: Context<'_, S>) -> String
    where
        S: Subscriber + for<'a> LookupSpan<'a>,
    {
        let timestamp = Local::now().format("%Y-%m-%d %H:%M:%S%.3f");
        let level = event.metadata().level();
        let target = event.metadata().target();
        
        // Collect fields
        let mut visitor = FieldCollector::new();
        event.record(&mut visitor);
        let fields = visitor.fields.join(", ");
        
        // Get span context if available
        let span_info = if let Some(scope) = ctx.event_span(event) {
            let span_name = scope.metadata().name();
            format!(" [{}]", span_name)
        } else {
            String::new()
        };
        
        let message = if fields.is_empty() {
            ""
        } else {
            &fields
        };
        
        format!("[{}] {} - {}{} - {}\n", timestamp, level, target, span_info, message)
    }
}

impl<S> Layer<S> for RollingFileLayer
where
    S: Subscriber + for<'a> LookupSpan<'a>,
{
    fn on_event(&self, event: &Event, ctx: Context<'_, S>) {
        let formatted = self.format_event(event, ctx);
        if let Err(e) = self.appender.write(&formatted) {
            eprintln!("Failed to write to rolling log file: {}", e);
        }
    }
}

/// Helper struct to collect event fields
struct FieldCollector {
    fields: Vec<String>,
}

impl FieldCollector {
    fn new() -> Self {
        FieldCollector {
            fields: Vec::new(),
        }
    }
}

impl Visit for FieldCollector {
    fn record_f64(&mut self, field: &Field, value: f64) {
        self.fields.push(format!("{}={}", field.name(), value));
    }
    
    fn record_i64(&mut self, field: &Field, value: i64) {
        self.fields.push(format!("{}={}", field.name(), value));
    }
    
    fn record_u64(&mut self, field: &Field, value: u64) {
        self.fields.push(format!("{}={}", field.name(), value));
    }
    
    fn record_bool(&mut self, field: &Field, value: bool) {
        self.fields.push(format!("{}={}", field.name(), value));
    }
    
    fn record_str(&mut self, field: &Field, value: &str) {
        self.fields.push(format!("{}=\"{}\"", field.name(), value));
    }
    
    fn record_error(&mut self, field: &Field, value: &(dyn std::error::Error + 'static)) {
        self.fields.push(format!("{}=\"{}\"", field.name(), value));
    }
    
    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        self.fields.push(format!("{}={:?}", field.name(), value));
    }
}

use std::sync::OnceLock;

// Global appender instance for backward compatibility
static GLOBAL_APPENDER: OnceLock<Arc<RollingFileAppender>> = OnceLock::new();

/// Initialize the rolling logger with tracing subscriber
pub fn init_logger(log_dir: PathBuf, app_name: &str) -> Result<(), Box<dyn std::error::Error>> {
    #[cfg(target_os = "android")]
    {
        android_logger::init_once(
            android_logger::Config::default()
                .with_max_level(log::LevelFilter::Trace)
                .with_tag(app_name),
        );
        println!("Android logger initialized for {}", app_name);
        return Ok(());
    }

    #[cfg(not(target_os = "android"))]
    {
        let appender = Arc::new(RollingFileAppender::new(log_dir)?);

        GLOBAL_APPENDER.set(appender.clone())
            .map_err(|_| "Logger already initialized")?;

        let file_layer = RollingFileLayer::new_with_appender(appender)?;

        tracing_subscriber::registry()
            .with(file_layer)
            .with(tracing_subscriber::fmt::layer().with_writer(std::io::stderr))
            .with(tracing_subscriber::filter::LevelFilter::INFO)
            .init();

        tracing::info!("Rolling logger with tracing initialized for {}", app_name);
        Ok(())
    }
}

impl RollingFileLayer {
    pub fn new_with_appender(appender: Arc<RollingFileAppender>) -> io::Result<Self> {
        Ok(RollingFileLayer { appender })
    }
}

/// Alternative initialization that allows custom subscriber setup
pub fn init_with_appender(log_dir: PathBuf) -> Result<Arc<RollingFileAppender>, Box<dyn std::error::Error>> {
    let appender = Arc::new(RollingFileAppender::new(log_dir)?);
    
    GLOBAL_APPENDER.set(appender.clone())
        .map_err(|_| "Logger already initialized")?;
    
    Ok(appender)
}

/// Get the global appender instance
fn get_appender() -> Option<&'static Arc<RollingFileAppender>> {
    GLOBAL_APPENDER.get()
}

/// Log an info message (backward compatibility)
pub fn info(message: &str) {
    tracing::info!("{}", message);
}

/// Log a debug message (backward compatibility)
pub fn debug(message: &str) {
    tracing::debug!("{}", message);
}

/// Log a warning message (backward compatibility)
pub fn warn(message: &str) {
    tracing::warn!("{}", message);
}

/// Log an error message (backward compatibility)
pub fn error(message: &str) {
    tracing::error!("{}", message);
}

/// Read current logs (backward compatibility)
pub fn read_logs() -> Result<String, String> {
    if let Some(appender) = get_appender() {
        appender.read_logs()
            .map_err(|e| format!("Failed to read logs: {}", e))
    } else {
        Err("Logger not initialized".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;
    
    #[test]
    fn test_rolling_logger_with_tracing() {
        let temp_dir = tempdir().unwrap();
        let appender = Arc::new(RollingFileAppender::new(temp_dir.path()).unwrap());
        
        // Test basic logging through tracing
        let formatted = "[test] INFO - Test info message\n";
        appender.write(formatted).unwrap();
        
        // Verify logs were written
        let content = appender.read_logs().unwrap();
        assert!(content.contains("Test info message"));
    }
    
    #[test]
    fn test_circular_buffer_with_tracing() {
        let temp_dir = tempdir().unwrap();
        let appender = Arc::new(RollingFileAppender::new(temp_dir.path()).unwrap());
        
        // Write enough data to definitely exceed the max size
        let large_message = "x".repeat(1000); // 1KB per message
        for i in 0..15000 { // 15MB total
            let formatted = format!("[test] INFO - Log entry {} - {}\n", i, large_message);
            appender.write(&formatted).unwrap();
        }
        
        // The logger should have wrapped around
        let content = appender.read_logs().unwrap();
        
        // Verify the logger handled the large volume without crashing
        assert!(content.len() > 0); // Should have content
        assert!(content.len() <= MAX_LOG_SIZE as usize); // Should not exceed max size
        
        // Verify recent entries are present
        assert!(content.contains("Log entry 14900")); // Should have recent entries
    }
    
    #[test]
    fn test_tracing_integration() {
        let temp_dir = tempdir().unwrap();
        let layer = RollingFileLayer::new(temp_dir.path()).unwrap();
        
        // This test just ensures the layer can be created and used
        // In a real scenario, you'd set up a full tracing subscriber
        assert!(layer.appender.current_size() == 0);
    }
}