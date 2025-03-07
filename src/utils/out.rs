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

#[macro_export]
macro_rules! update_print {
    // 带格式化参数的版本
    ($fmt:expr, $($arg:tt)*) => {{
        use std::io::Write;
        print!("\r{}", format!($fmt, $($arg)*));
        std::io::stdout().flush().unwrap();
    }};

    // 不带格式化参数的版本
    ($msg:expr) => {{
        use std::io::Write;
        print!("\r{}", $msg);
        std::io::stdout().flush().unwrap();
    }};
}

/// 在终端的同一行更新打印内容
/// 通过使用回车符 (\r) 将光标移动到行首，从而覆盖之前打印的内容
///
/// # Arguments
/// * `msg` - 要打印的消息内容
///
/// # Example
/// ```ignore
/// update_line_print("Loading... 50%");
/// // 稍后
/// update_line_print("Loading... 100%");
/// ```
#[inline]
#[allow(dead_code)]
pub fn update_line_print(msg: &str) {
    update_print!("{}", msg);
}

/// 进度条的默认宽度
pub const PROGRESS_BAR_WIDTH: usize = 30;

#[macro_export]
macro_rules! print_progress {
    // 基本用法
    ($current:expr, $total:expr) => {
        print_progress!($current, $total, crate::utils::out::PROGRESS_BAR_WIDTH, "")
    };

    // 指定宽度
    ($current:expr, $total:expr, $width:expr) => {
        print_progress!($current, $total, $width, "")
    };

    // 带额外消息的版本
    ($current:expr, $total:expr, $width:expr, $fmt:expr, $($arg:tt)*) => {{
        use std::io::Write;

        let progress = ($current as f64 / $total as f64).clamp(0.0, 1.0);
        let filled_len = (progress * $width as f64) as usize;
        let empty_len = $width - filled_len;

        let bar = "█".repeat(filled_len) + &"░".repeat(empty_len);
        let percentage = (progress * 100.0) as usize;

        // 保存光标位置，清除从光标到屏幕底部的内容
        print!("\x1B[s\x1B[J");  // 保存位置并清除之后的所有行
        print!("[{}] {:>3}% ({}/{})\n{}",
            bar,
            percentage,
            $current,
            $total,
            format!($fmt, $($arg)*)
        );
        // 恢复光标位置
        print!("\x1B[u");
        std::io::stdout().flush().unwrap();

        // 如果进度完成，移动到消息下方并打印换行
        if $current >= $total {
            print!("\n\n");
            std::io::stdout().flush().unwrap();
        }
    }};

    // 带额外消息但不需要格式化的版本
    ($current:expr, $total:expr, $width:expr, $msg:expr) => {{
        use std::io::Write;

        let progress = ($current as f64 / $total as f64).clamp(0.0, 1.0);
        let filled_len = (progress * $width as f64) as usize;
        let empty_len = $width - filled_len;

        let bar = "█".repeat(filled_len) + &"░".repeat(empty_len);
        let percentage = (progress * 100.0) as usize;

        // 保存光标位置，清除从光标到屏幕底部的内容
        print!("\x1B[s\x1B[J");  // 保存位置并清除之后的所有行
        print!("[{}] {:>3}% ({}/{})\n{}",
            bar,
            percentage,
            $current,
            $total,
            $msg
        );
        // 恢复光标位置
        print!("\x1B[u");
        std::io::stdout().flush().unwrap();

        // 如果进度完成，移动到消息下方并打印换行
        if $current >= $total {
            print!("\n\n");
            std::io::stdout().flush().unwrap();
        }
    }};
}

#[cfg(test)]
mod tests {
    use super::*;

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

    #[test]
    fn test_update_line_print() {
        for i in 0..10 {
            update_line_print(&format!("Loading... {}", i));
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
        println!();
    }

    #[test]
    fn test_update_line_print_macro() {
        // 测试不带格式化参数的版本
        update_line_print("Simple message");
        std::thread::sleep(std::time::Duration::from_millis(500));

        // 测试带格式化参数的版本
        for i in 0..10 {
            update_print!("Progress: {} %", i);
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
        println!();
    }

    #[test]
    fn test_progress_bar() {
        let total = 100;
        for i in 0..=total {
            print_progress!(i, total);
            std::thread::sleep(std::time::Duration::from_millis(50));
        }

        // 测试自定义宽度
        println!("\n测试自定义宽度的进度条：");
        let total = 50;
        for i in 0..=total {
            print_progress!(i, total, 20);
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
    }

    #[test]
    fn test_progress_bar_with_message() {
        println!("\n测试带消息的进度条：");
        let total = 50;
        for i in 0..=total {
            // 测试带格式化参数的消息
            print_progress!(i, total, 30, "正在处理文件：file_{}.txt", i);
            std::thread::sleep(std::time::Duration::from_millis(50));
        }

        println!("\n测试带静态消息的进度条：");
        for i in 0..=total {
            // 测试静态消息
            print_progress!(i, total, 30, "正在进行某项操作...");
            std::thread::sleep(std::time::Duration::from_millis(50));
        }

        // 测试不同进度阶段显示不同消息
        println!("\n测试动态消息的进度条：");
        for i in 0..=total {
            let message = match (i as f64 / total as f64 * 100.0) as usize {
                0..=25 => "初始化中...",
                26..=50 => "加载数据...",
                51..=75 => "处理数据...",
                _ => "完成处理...",
            };
            print_progress!(i, total, 30, "{}", message);
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
    }
}
