use clap::Parser;
use rust_tier::config::Config;
use tracing::{debug, error, info, trace, warn};
use tracing_subscriber::{EnvFilter, fmt, layer::SubscriberExt, reload};

#[derive(clap::Parser, Debug)]
#[command(version, about = "Rust Tier Daemon")]
struct Args {
    #[arg(short = 'c', long = "config", default_value = "config.yml")]
    config: String,
}
#[tokio::main]
async fn main() {
    {  
        // 读取配置文件，更新日志等级。
        let filter = EnvFilter::new("trace");
        let (filter_layer, reload_handle) = reload::Layer::new(filter);

        let subscriber = tracing_subscriber::registry()
            .with(filter_layer)
            .with(fmt::Layer::default());

        tracing::subscriber::set_global_default(subscriber).unwrap();

        let args = Args::parse();

        let config = match Config::new(&args.config) {
            Ok(config) => {
                info!("Config file loaded");
                config
            }
            Err(err) => {
                error!("Config file load failed: {}", err);
                panic!()
            }
        };

        reload_handle
            .reload(EnvFilter::new(&config.log.level))
            .unwrap();
    }
}
