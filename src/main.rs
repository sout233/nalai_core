use std::{io, thread, time::Duration};

use salvo::prelude::*;
use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};
use utils::global_wrappers;
use tracing::info;

mod handlers;
mod models;
mod utils;


#[tokio::main]
async fn main() {
    let _guard = init_tracing(); // 调用此函数时会返回一个守护进程，确保程序结束前日志被刷新

    info!("Starting server");

    // global_wrappers::load_global_wrappers_from_json().await;
    global_wrappers::load_global_wrappers_from_sled().await;

    tokio::spawn(async {
        let router = Router::new()
            .push(
                Router::with_path("/download")
                    .post(handlers::download::start_download_api)
                    .delete(handlers::download::delete_download_api),
            )
            .push(Router::with_path("/info").get(handlers::info::get_info_api))
            .push(Router::with_path("/cancel").post(handlers::download::cancel_download_api))
            .push(Router::with_path("/all_info").get(handlers::info::get_all_info_api))
            .push(Router::with_path("/sorc").post(handlers::download::cancel_or_start_download_api))
            .push(Router::with_path("checkhealth").get(handlers::health::check_health_api))
            .push(Router::with_path("exit").get(handlers::exit::exit_api))
            .push(Router::with_path("ws").goal(handlers::ws::connect));

        let acceptor = TcpListener::new("127.0.0.1:13088").bind().await;

        Server::new(acceptor).serve(router).await;
    });

    // ctrlc::set_handler(async || {
    //     println!("收到中断信号，正在保存数据...");
    //     if let Err(e) = save_to_file().await {
    //         eprintln!("保存数据时出错: {}", e);
    //     } else {
    //         println!("数据已成功保存");
    //     }
    //     std::process::exit(0);
    // }).unwrap();

    info!("Nalai Core 服务已启动 ヾ(≧▽≦*)o");

    loop {
        thread::sleep(Duration::from_secs(1));
    }
}


fn init_tracing() -> WorkerGuard {
    // 创建或打开一个文件用于写入日志
    let file_appender = tracing_appender::rolling::daily("./logs", "nalai_core.log");
    let (non_blocking, guard) = tracing_appender::non_blocking(file_appender);

    // 设置环境过滤器，默认为 info 级别
    let env_filter = EnvFilter::try_from_default_env()
        .or_else(|_| EnvFilter::try_new("info"))
        .unwrap();

    // 配置文件格式化输出
    let file_layer = fmt::layer()
        .with_writer(non_blocking)
        .with_ansi(false) // 关闭 ANSI 颜色编码，适用于文件输出
        .with_level(true) // 显示日志级别
        .with_target(true) // 显示目标模块
        .with_thread_ids(true)
        .pretty(); // 显示线程 ID

    // 配置控制台格式化输出
    let console_layer = fmt::layer()
        .with_writer(io::stdout)
        .with_ansi(true) // 控制台通常支持 ANSI 颜色编码
        .with_level(true)
        .with_target(true)
        .with_thread_ids(true);

    // 组合过滤器和格式化层
    tracing_subscriber::registry()
        .with(env_filter)
        .with(file_layer)
        .with(console_layer)
        .init();

    guard
}