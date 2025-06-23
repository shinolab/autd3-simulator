use std::sync::Arc;

use autd3_protobuf::{
    CloseRequest, CloseResponse, FromMessage, Geometry, GeometryResponse, ReadRequest, RxMessage,
    SendResponse, TxRawData, simulator_server,
};
use parking_lot::RwLock;
use tonic::{Request, Response, Status};
use winit::event_loop::EventLoopProxy;

use crate::event::{Signal, UserEvent};

pub struct SimulatorServer {
    pub rx_buf: Arc<RwLock<Vec<autd3_core::link::RxMessage>>>,
    pub proxy: EventLoopProxy<UserEvent>,
}

#[tonic::async_trait]
impl simulator_server::Simulator for SimulatorServer {
    async fn config_geometry(
        &self,
        req: Request<Geometry>,
    ) -> Result<Response<GeometryResponse>, Status> {
        let geometry = autd3_driver::geometry::Geometry::from_msg(req.into_inner())?;
        if self
            .proxy
            .send_event(UserEvent::Server(Signal::ConfigGeometry(geometry)))
            .is_err()
        {
            return Err(Status::unavailable("Simulator is closed"));
        }
        Ok(Response::new(GeometryResponse {}))
    }

    async fn update_geometry(
        &self,
        req: Request<Geometry>,
    ) -> Result<Response<GeometryResponse>, Status> {
        let geometry = autd3_driver::geometry::Geometry::from_msg(req.into_inner())?;
        if self
            .proxy
            .send_event(UserEvent::Server(Signal::UpdateGeometry(geometry)))
            .is_err()
        {
            return Err(Status::unavailable("Simulator is closed"));
        }
        Ok(Response::new(GeometryResponse {}))
    }

    async fn send_data(&self, req: Request<TxRawData>) -> Result<Response<SendResponse>, Status> {
        let tx = Vec::<autd3_core::link::TxMessage>::from_msg(req.into_inner())?;
        if self
            .proxy
            .send_event(UserEvent::Server(Signal::Send(tx)))
            .is_err()
        {
            return Err(Status::unavailable("Simulator is closed"));
        }
        Ok(Response::new(SendResponse {}))
    }

    async fn read_data(&self, _: Request<ReadRequest>) -> Result<Response<RxMessage>, Status> {
        let rx = self.rx_buf.read();
        Ok(Response::new(RxMessage {
            data: rx
                .iter()
                .flat_map(|c| [c.data(), c.ack().into_bits()])
                .collect(),
        }))
    }

    async fn close(&self, _: Request<CloseRequest>) -> Result<Response<CloseResponse>, Status> {
        if self
            .proxy
            .send_event(UserEvent::Server(Signal::Close))
            .is_err()
        {
            return Err(Status::unavailable("Simulator is closed"));
        }
        Ok(Response::new(CloseResponse {}))
    }
}
