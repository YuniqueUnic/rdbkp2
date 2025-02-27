#!/bin/bash
set -e  # 遇到错误立即退出

# 颜色输出函数
GREEN='\033[0;32m'
RED='\033[0;31m'
NC='\033[0m'

log_success() {
    echo -e "${GREEN}[SUCCESS]${NC} $1"
}

log_error() {
    echo -e "${RED}[ERROR]${NC} $1"
    exit 1
}

# 检查 rdbkp2 是否安装
if ! command -v rdbkp2 &> /dev/null; then
    log_error "rdbkp2 command not found. Please install it first."
fi

# 清理旧的测试环境
echo "Cleaning up old test environment..."
docker-compose down -v &> /dev/null || true
rm -rf ./data/* &> /dev/null || true

# 启动服务
echo "Starting services..."
docker-compose up -d
sleep 5

# 检查服务健康状态
echo "Checking service health..."
if ! curl -s http://localhost:8666 &> /dev/null; then
    log_error "Service is not healthy"
fi
log_success "Service is healthy"

# 生成测试数据
echo "Generating test data..."
for i in {1..10}; do
    curl -s "http://localhost:8666/test?id=$i"
    sleep 1
done

# 验证日志文件
echo "Validating log file..."
if [ ! -f "./data/log.txt" ]; then
    log_error "Log file not created"
fi

LOG_COUNT=$(wc -l < "./data/log.txt")
if [ "$LOG_COUNT" -lt 10 ]; then
    log_error "Expected at least 10 log entries, but got $LOG_COUNT"
fi
log_success "Log file validated successfully"

# 备份前保存日志内容
echo "Saving original log content..."
cp ./data/log.txt ./data/original_log.txt

# 停止服务并执行备份
echo "Stopping service and performing backup..."
docker-compose stop
rdbkp2 backup -c sim-server -o ./backups || log_error "Backup failed"
log_success "Backup completed"

# 修改服务配置
echo "Modifying service configuration..."
sed -i 's/Welcome to SimServer!/Welcome to Updated SimServer!/g' config.yaml
sed -i 's/"1.0"/"2.0"/g' config.yaml

# 清空数据目录
echo "Clearing data directory..."
rm -rf ./data/*

# 恢复备份
echo "Restoring backup..."
rdbkp2 restore -c sim-server -i ./backups/latest || log_error "Restore failed"
log_success "Restore completed"

# 重启服务
echo "Restarting service..."
docker-compose up -d
sleep 5

# 验证服务恢复
echo "Validating service restoration..."
RESPONSE=$(curl -s http://localhost:8666)
if ! echo "$RESPONSE" | grep -q "Updated SimServer"; then
    log_error "Service configuration not updated properly"
fi

# 验证日志恢复
echo "Validating log restoration..."
if ! diff -q "./data/log.txt" "./data/original_log.txt" &> /dev/null; then
    log_error "Restored log file differs from original"
fi
log_success "Log file restored successfully"

# 添加新的测试数据
echo "Adding new test data..."
for i in {11..15}; do
    curl -s "http://localhost:8666/test?id=$i"
    sleep 1
done

# 验证新旧数据整合
echo "Validating data integration..."
NEW_LOG_COUNT=$(wc -l < "./data/log.txt")
if [ "$NEW_LOG_COUNT" -lt 15 ]; then
    log_error "Expected at least 15 log entries after adding new data, but got $NEW_LOG_COUNT"
fi
log_success "Data integration validated"

# 清理测试环境
echo "Cleaning up test environment..."
docker-compose down
rm -rf ./backups/* ./data/original_log.txt

log_success "All tests completed successfully!" 