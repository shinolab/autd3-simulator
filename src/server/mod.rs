mod grpc;

use crate::error::Result;
use crate::event::UserEvent;
use parking_lot::RwLock;
use tokio::runtime::Runtime;
use winit::event_loop::EventLoopProxy;

use std::sync::Arc;

use autd3_core::link::RxMessage;
use std::net::ToSocketAddrs;
use tokio::sync::oneshot;
use tokio::task::JoinHandle;

#[allow(clippy::type_complexity)]
pub struct Server {
    server_th: JoinHandle<Result<()>>,
    shutdown: oneshot::Sender<()>,
}

impl Server {
    pub fn new(
        runtime: &Runtime,
        port: u16,
        rx_buf: Arc<RwLock<Vec<RxMessage>>>,
        proxy: EventLoopProxy<UserEvent>,
    ) -> Result<Self> {
        let (sender_shutdown, receiver_shutdown) = oneshot::channel::<()>();

        let server_th = runtime.spawn({
            async move {
                tonic::transport::Server::builder()
                    .add_service(autd3_protobuf::simulator_server::SimulatorServer::new(
                        grpc::SimulatorServer { rx_buf, proxy },
                    ))
                    .serve_with_shutdown(
                        format!("0.0.0.0:{port}")
                            .to_socket_addrs()
                            .unwrap()
                            .next()
                            .unwrap(),
                        async move {
                            let _ = receiver_shutdown.await;
                        },
                    )
                    .await?;
                Ok(())
            }
        });

        Ok(Self {
            server_th,
            shutdown: sender_shutdown,
        })
    }

    pub async fn shutdown(self) -> Result<()> {
        let Self {
            server_th,
            shutdown,
            ..
        } = self;
        let _ = shutdown.send(());
        server_th.await?
    }
}
