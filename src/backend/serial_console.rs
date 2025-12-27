//! Serial console utilities for VM log parsing
//!
//! Provides parsing and analysis of serial console logs,
//! including BiomeOS BootLogger output.

use std::path::Path;

/// Parse BiomeOS BootLogger output to check if boot is complete
pub fn is_boot_complete(log_content: &str) -> bool {
    log_content.contains("BiomeOS Init Complete") || log_content.contains("[Success]")
}

/// Extract boot time from BootLogger output
///
/// Looks for patterns like "BiomeOS Init Complete (178ms)"
pub fn parse_boot_time_ms(log_content: &str) -> Option<u64> {
    for line in log_content.lines() {
        if line.contains("BiomeOS Init Complete") {
            // Extract time from pattern like "(178ms)"
            if let Some(start) = line.find('(') {
                if let Some(end) = line.find("ms)") {
                    let time_str = &line[start + 1..end];
                    if let Ok(ms) = time_str.parse() {
                        return Some(ms);
                    }
                }
            }
        }
    }
    None
}

/// Count log entries by severity
pub struct LogStats {
    /// Number of info-level messages
    pub info_count: usize,
    /// Number of warning messages
    pub warn_count: usize,
    /// Number of error messages
    pub error_count: usize,
    /// Number of success messages
    pub success_count: usize,
}

/// Analyze serial console log for statistics
pub fn analyze_log(log_content: &str) -> LogStats {
    let mut stats = LogStats {
        info_count: 0,
        warn_count: 0,
        error_count: 0,
        success_count: 0,
    };

    for line in log_content.lines() {
        if line.contains("[Info]") {
            stats.info_count += 1;
        } else if line.contains("[Warn]") {
            stats.warn_count += 1;
        } else if line.contains("[Error]") {
            stats.error_count += 1;
        } else if line.contains("[Success]") {
            stats.success_count += 1;
        }
    }

    stats
}

/// Extract all error messages from log
pub fn extract_errors(log_content: &str) -> Vec<String> {
    log_content
        .lines()
        .filter(|line| line.contains("[Error]"))
        .map(|line| line.to_string())
        .collect()
}

/// Read and parse serial console log file
pub async fn read_serial_log(path: &Path) -> std::io::Result<String> {
    tokio::fs::read_to_string(path).await
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE_LOG: &str = r#"[2025-12-27 10:23:45] [Info] BiomeOS Init Starting
[2025-12-27 10:23:45] [Info] Filesystem: rootfs mounted (rw)
[2025-12-27 10:23:46] [Info] Network: eth0 configured (10.42.0.10/24)
[2025-12-27 10:23:47] [Warn] Optional service skipped
[2025-12-27 10:23:48] [Info] Primal: Songbird started (PID 234)
[2025-12-27 10:23:49] [Success] BiomeOS Init Complete (178ms)"#;

    #[test]
    fn test_is_boot_complete() {
        assert!(is_boot_complete(SAMPLE_LOG));
        assert!(!is_boot_complete("[Info] BiomeOS Init Starting"));
    }

    #[test]
    fn test_parse_boot_time() {
        assert_eq!(parse_boot_time_ms(SAMPLE_LOG), Some(178));
        assert_eq!(parse_boot_time_ms("[Info] Starting..."), None);
    }

    #[test]
    fn test_analyze_log() {
        let stats = analyze_log(SAMPLE_LOG);
        assert_eq!(stats.info_count, 4);
        assert_eq!(stats.warn_count, 1);
        assert_eq!(stats.error_count, 0);
        assert_eq!(stats.success_count, 1);
    }

    #[test]
    fn test_extract_errors() {
        let log_with_error = "[Error] Something failed\n[Info] Normal log";
        let errors = extract_errors(log_with_error);
        assert_eq!(errors.len(), 1);
        assert!(errors[0].contains("Something failed"));
    }
}
