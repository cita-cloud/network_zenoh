# Network Zenoh Agent Specification

## 1. 项目基本信息 (Project Basic Information)

- **项目名称**: network_zenoh (Binary: `network`)
- **版本**: 6.7.5
- **描述**: CITA-Cloud 区块链平台的网络微服务组件，基于 Eclipse Zenoh 协议实现。负责节点间的对等通信、消息路由以及与 CITA-Cloud 其他微服务的交互。
- **仓库地址**: https://github.com/cita-cloud/network_zenoh
- **开发语言**: Rust (Edition 2021)

## 2. 核心能力 (Core Capabilities)

### 2.1 节点通信 (Peer-to-Peer Communication)
- 基于 Zenoh 协议（支持 QUIC/TLS）实现高效的节点间通信。
- 支持节点发现、连接管理和链路保活（Keep-alive）。
- 支持自定义接收缓冲区大小 (`rx_buffer_size`) 以适应高吞吐场景。

### 2.2 消息路由与分发 (Message Routing & Dispatching)
- 提供 GRPC 接口 (`NetworkService`) 供其他组件调用。
- 实现消息的入站（Inbound）和出站（Outbound）分发。
- 支持多模块配置，根据配置将消息路由到指定的微服务端口。

### 2.3 配置热更新 (Hot Configuration Update)
- 支持在不重启服务的情况下动态更新配置（如节点列表）。
- 可配置热更新检查间隔 (`hot_update_interval`)。

### 2.4 可观测性与健康检查 (Observability & Health Check)
- **Metrics**: 内置 Prometheus Metrics 导出器，支持自定义 Buckets。
- **Tracing**: 集成 OpenTelemetry Tracing，支持日志分级和格式化。
- **Health Check**: 提供 GRPC 健康检查接口，并定期检测下游模块的健康状态。

### 2.5 安全性 (Security)
- 支持基于 CA 证书的 TLS 加密通信。
- 配置项包含 CA 证书、服务端证书及私钥。

## 3. 运行依赖 (Runtime Dependencies)

### 3.1 环境依赖
- **OS**: Linux (推荐)
- **Rust Toolchain**: Stable (最新版)
- **External Libs**: `protoc` (用于编译 proto 文件)

### 3.2 配置文件
运行需要提供 TOML 格式的配置文件（默认 `config.toml`），包含以下关键部分：
- **网络配置**: 监听端口、协议、域名。
- **证书配置**: CA、Cert、Key。
- **对等节点 (Peers)**: 其他节点的连接信息。
- **模块配置 (Modules)**: CITA-Cloud 其他组件的地址映射。

## 4. 使用示例 (Usage Examples)

### 4.1 编译项目
```bash
cargo build --release
```

### 4.2 运行服务
```bash
# 使用默认配置文件 config.toml 运行
./target/release/network run

# 指定配置文件运行
./target/release/network run -c /path/to/your/config.toml
```

### 4.3 配置文件示例 (`config.toml`)
```toml
grpc_port = 50000
protocol = "quic"
port = 40000
domain = "node1"
ca_cert = "..."
cert = "..."
priv_key = "..."
enable_metrics = true
metrics_port = 60000

[[peers]]
protocol = "quic"
port = 40001
domain = "node2"

[[modules]]
module_name = "consensus"
hostname = "127.0.0.1"
port = 50001
```

### 4.4 命令行帮助
```bash
./target/release/network --help
```
