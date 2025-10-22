mod custom;

use crate::error::Result;
use crate::event::UserEvent;
use winit::event_loop::EventLoopProxy;

use std::net::TcpListener;
use std::sync::{Arc, RwLock};
use std::thread::{self, JoinHandle};

use autd3_core::link::RxMessage;

pub struct Server {
    _server_th: JoinHandle<Result<()>>,
}

impl Server {
    pub fn new(
        port: u16,
        rx_buf: Arc<RwLock<Vec<RxMessage>>>,
        proxy: EventLoopProxy<UserEvent>,
    ) -> Result<Self> {
        let server_th = thread::spawn(move || {
            let listener = TcpListener::bind(format!("0.0.0.0:{port}"))?;
            println!("listening on port {}", port);

            custom::CustomServer::new(rx_buf, proxy).run(listener)?;
            Ok(())
        });

        Ok(Self {
            _server_th: server_th,
        })
    }

    pub fn shutdown(self) -> Result<()> {
        Ok(())
    }
}
