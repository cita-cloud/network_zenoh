# network_zenoh

`CITA-Cloud`中[network微服务](https://github.com/cita-cloud/cita_cloud_proto/blob/master/protos/network.proto)的实现，基于[zenoh](https://crates.io/crates/zenoh)。

## 编译docker镜像
```
docker build -t citacloud/network_zenoh .
```

## 使用方法

```
$ network -h
network 6.6.3
Rivtower Technologies <contact@rivtower.com>
This doc string acts as a help message when the user runs '--help' as do all doc strings on fields

USAGE:
    network <SUBCOMMAND>

OPTIONS:
    -h, --help       Print help information
    -V, --version    Print version information

SUBCOMMANDS:
    help    Print this message or the help of the given subcommand(s)
    run     run this service

```

### network-run

运行`network`服务。

```
$ network run -h
network-run 
run this service

USAGE:
    network run [OPTIONS]

OPTIONS:
    -c, --config <CONFIG_PATH>    Chain config path [default: config.toml]
    -h, --help                    Print help information

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
```

## 设计

