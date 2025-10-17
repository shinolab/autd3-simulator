// mod grpc;
mod custom;

use crate::error::Result;
use crate::event::UserEvent;
use parking_lot::RwLock;
use tokio::runtime::Runtime;
use winit::event_loop::EventLoopProxy;

use std::sync::Arc;

use autd3_core::link::RxMessage;
use tokio::net::TcpListener;
use tokio::task::JoinHandle;

#[allow(clippy::type_complexity)]
pub struct Server {
    server_th: JoinHandle<Result<()>>,
}

impl Server {
    pub fn new(
        runtime: &Runtime,
        port: u16,
        rx_buf: Arc<RwLock<Vec<RxMessage>>>,
        proxy: EventLoopProxy<UserEvent>,
    ) -> Result<Self> {
        let server_th = runtime.spawn({
            async move {
                let listener = TcpListener::bind(format!("0.0.0.0:{port}")).await?;
                tracing::info!("listening on port {}", port);

                custom::CustomServer::new(rx_buf, proxy)
                    .run(listener)
                    .await?;
                Ok(())
            }
        });

        Ok(Self { server_th })
    }

    pub async fn shutdown(self) -> Result<()> {
        let Self { server_th } = self;
        server_th.abort();
        Ok(())
    }
}
