#[macro_export]
macro_rules! log_bail {
    // 带格式化参数的版本
    ($level:expr, $fmt:expr, $($arg:tt)*) => {{
        let level = $level.to_string();
        let level = level.to_uppercase();
        match level.as_str() {
            "ERROR" => tracing::error!($fmt, $($arg)*),
            "WARN" => tracing::warn!($fmt, $($arg)*),
            "INFO" => tracing::info!($fmt, $($arg)*),
            "DEBUG" => tracing::debug!($fmt, $($arg)*),
            "TRACE" => tracing::trace!($fmt, $($arg)*),
            _ => tracing::debug!($fmt, $($arg)*),
        }
        println!($fmt, $($arg)*);
        anyhow::bail!($fmt, $($arg)*);

    }};

    // 不带格式化参数的版本
    ($level:expr, $msg:expr) => {{
        let level = $level.to_string();
        let level = level.to_uppercase();
        match level.as_str() {
            "ERROR" => tracing::error!($msg),
            "WARN" => tracing::warn!($msg),
            "INFO" => tracing::info!($msg),
            "DEBUG" => tracing::debug!($msg),
            "TRACE" => tracing::trace!($msg),
            _ => tracing::debug!($msg),
        }
        println!($msg);
        anyhow::bail!($msg);
    }};
}

#[macro_export]
macro_rules! log_println {
    // 带格式化参数的版本
    ($level:expr, $fmt:expr, $($arg:tt)*) => {{
        let level = $level.to_string();
        let level = level.to_uppercase();
        match level.as_str() {
            "ERROR" => tracing::error!($fmt, $($arg)*),
            "WARN" => tracing::warn!($fmt, $($arg)*),
            "INFO" => tracing::info!($fmt, $($arg)*),
            "DEBUG" => tracing::debug!($fmt, $($arg)*),
            "TRACE" => tracing::trace!($fmt, $($arg)*),
            _ => tracing::debug!($fmt, $($arg)*),
        }
        println!($fmt, $($arg)*);
        // anyhow::bail!($fmt, $($arg)*);
    }};

    // 不带格式化参数的版本
    ($level:expr, $msg:expr) => {{
        let level = $level.to_string();
        let level = level.to_uppercase();
        match level.as_str() {
            "ERROR" => tracing::error!($msg),
            "WARN" => tracing::warn!($msg),
            "INFO" => tracing::info!($msg),
            "DEBUG" => tracing::debug!($msg),
            "TRACE" => tracing::trace!($msg),
            _ => tracing::debug!($msg),
        }
        println!($msg);
        // anyhow::bail!($msg);
    }};
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_log_bail() {
        // 使用 try block 来捕获错误
        let res = (|| -> anyhow::Result<()> { log_bail!("ERROR", "test") })();

        // 验证确实返回了错误
        assert!(res.is_err());
        // 验证错误消息
        assert_eq!(res.unwrap_err().to_string(), "test");
    }

    #[test]
    fn test_log_print() {
        log_println!("ERROR", "test");
    }
}
