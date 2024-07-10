# network_zenoh

`CITA-Cloud`中[network微服务](https://github.com/cita-cloud/cita_cloud_proto/blob/master/protos/network.proto)的实现，基于[zenoh](https://crates.io/crates/zenoh)。

## 编译docker镜像
```
docker build -t citacloud/network_zenoh .
```

## 使用方法

```
$ network -h
network 6.7.0
Rivtower Technologies <contact@rivtower.com>

Usage: network <COMMAND>

Commands:
  run   run this service
  help  Print this message or the help of the given subcommand(s)

Options:
  -h, --help     Print help
  -V, --version  Print version
```

### network-run

运行`network`服务。

```
$ network run -h
run this service

Usage: network run [OPTIONS]

Options:
  -c, --config <CONFIG_PATH>  Chain config path [default: config.toml]
  -h, --help                  Print help

```

参数：
1. `config` 微服务配置文件。

    参见示例`example/config.toml`。

    其中`[network_zenoh]`
    * `ca_cert` 为`CA`根证书。
    * `cert` 为节点证书。
    * `priv_key` 为节点证书对应的私钥。
    * `grpc_port` 为`gRPC`服务监听的端口号。
    * `protocol` 为`zenoh`服务协议类型：`tls`, `tcp`, `quic`
    * `domain` 为域名
    * `port` 为`zenoh`服务监听端口
    * `peers` 为`zenoh`邻居节点的网络信息，其中`protocol`字段为服务协议类型，`port`字段为端口号，`domain`字段为该邻居节点申请证书时使用的域名。
    * `chain_id` 链的唯一标识
    * `node_address` 节点地址文件路径
    * `validator_address` 共识节点地址文件路径
    * `modules` 为同节点的其它微服务网络信息，其中`module_name`字段为模块名称，`port`字段为该微服务的grpc端口号，`hostname`字段为该微服务的网络地址。
    * `hot_update_interval` 节点配置热更新间隔时间（以秒为单位）
    * `health_check_timeout` 健康检查超时时间（以秒为单位）
    * `rx_buffer_size` 每个链接的接收缓冲区大小（以字节为单位）

    其中`[network_zenoh.log_config]`段为微服务日志的配置：
    * `max_level` 日志等级
    * `filter` 日志过滤配置
    * `service_name` 服务名称，用作日志文件名与日志采集的服务名称
    * `rolling_file_path` 日志文件路径
    * `agent_endpoint` jaeger 采集端地址


```
$ network run -c example/config.toml

2024-07-10T10:56:04.120025+08:00  INFO network: grpc port of network_zenoh: 50000
2024-07-10T10:56:04.120429+08:00  INFO network: start network_zenoh grpc server!
2024-07-10T10:56:04.120432+08:00  INFO network: metrics on
2024-07-10T10:56:04.120764+08:00  INFO network::server: ZenohId: b350db8dedbbd3818c9566b103829f7f
2024-07-10T10:56:04.1222+08:00  INFO zenoh::net::runtime: Using ZID: b350db8dedbbd3818c9566b103829f7f
2024-07-10T10:56:04.124933+08:00  INFO zenoh::net::runtime::orchestrator: Zenoh can be reached at: quic/198.18.0.1:40000
...
2024-07-10T10:56:04.124985+08:00  INFO zenoh::net::runtime::orchestrator: Zenoh can be reached at: quic/[fe80::1]:40000
2024-07-10T10:56:04.62724+08:00  WARN zenoh::net::runtime::orchestrator: Scouting delay elapsed before start conditions are met.
2024-07-10T10:56:04.629143+08:00  INFO network::server: Peer connected, new alive token (test-chain-0 - 14075586594044653895)

```

## 设计

