// # Protocol Specification
//
// This link uses a simple TCP-based binary protocol for communication with the simulator.
//
// ## Message Types
//
// - `0x01`: Configure Geometry
// - `0x02`: Update Geometry
// - `0x03`: Send Data
// - `0x04`: Read Data
// - `0x05`: Close
//
// ## Message Formats
//
// ### Configure/Update Geometry
// Request:
// - 1 byte: message type (0x01 or 0x02)
// - 4 bytes: number of devices (u32, little-endian)
// - For each device:
//   - 12 bytes: position (3x f32, little-endian)
//   - 16 bytes: rotation quaternion (w, i, j, k as f32, little-endian)
//
// Response:
// - 1 byte: status (0x00 = OK)
//
// ### Send Data
// Request:
// - 1 byte: message type (0x03)
// - 4 bytes: number of devices (u32, little-endian)
// - Raw TxMessage data for each device
//
// Response:
// - 1 byte: status (0x00 = OK)
//
// ### Read Data
// Request:
// - 1 byte: message type (0x04)
//
// Response:
// - 1 byte: status (0x00 = OK)
// - 4 bytes: number of devices (u32, little-endian)
// - Raw RxMessage data for each device
//
// ### Close
// Request:
// - 1 byte: message type (0x05)
//
// Response:
// - 1 byte: status (0x00 = OK)

use std::sync::Arc;

use autd3_core::link::{RxMessage, TxMessage};
use autd3_driver::geometry::Geometry;
use parking_lot::RwLock;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use winit::event_loop::EventLoopProxy;

use crate::error::Result;
use crate::event::{Signal, UserEvent};

const MSG_CONFIG_GEOMETRY: u8 = 0x01;
const MSG_UPDATE_GEOMETRY: u8 = 0x02;
const MSG_SEND_DATA: u8 = 0x03;
const MSG_READ_DATA: u8 = 0x04;
const MSG_CLOSE: u8 = 0x05;

const STATUS_OK: u8 = 0x00;

pub struct CustomServer {
    pub(crate) rx_buf: Arc<RwLock<Vec<RxMessage>>>,
    pub(crate) proxy: EventLoopProxy<UserEvent>,
}

unsafe impl Send for CustomServer {}
unsafe impl Sync for CustomServer {}

impl CustomServer {
    pub fn new(rx_buf: Arc<RwLock<Vec<RxMessage>>>, proxy: EventLoopProxy<UserEvent>) -> Self {
        Self { rx_buf, proxy }
    }

    async fn handle_client(&self, mut stream: TcpStream) -> Result<()> {
        loop {
            let mut msg_type = [0u8; 1];
            if stream.read_exact(&mut msg_type).await.is_err() {
                break;
            }

            match msg_type[0] {
                MSG_CONFIG_GEOMETRY => {
                    self.handle_config_geometry(&mut stream).await?;
                }
                MSG_UPDATE_GEOMETRY => {
                    self.handle_update_geometry(&mut stream).await?;
                }
                MSG_SEND_DATA => {
                    self.handle_send_data(&mut stream).await?;
                }
                MSG_READ_DATA => {
                    self.handle_read_data(&mut stream).await?;
                }
                MSG_CLOSE => {
                    self.handle_close(&mut stream).await?;
                    break;
                }
                _ => {
                    tracing::warn!("Unknown message type: {}", msg_type[0]);
                    break;
                }
            }
        }
        Ok(())
    }

    async fn handle_config_geometry(&self, stream: &mut TcpStream) -> Result<()> {
        let geometry = self.read_geometry(stream).await?;
        if self
            .proxy
            .send_event(UserEvent::Server(Signal::ConfigGeometry(geometry)))
            .is_err()
        {
            return Err(crate::error::SimulatorError::ServerError(
                "Simulator is closed".to_string(),
            ));
        }
        stream.write_all(&[STATUS_OK]).await?;
        Ok(())
    }

    async fn handle_update_geometry(&self, stream: &mut TcpStream) -> Result<()> {
        let geometry = self.read_geometry(stream).await?;
        if self
            .proxy
            .send_event(UserEvent::Server(Signal::UpdateGeometry(geometry)))
            .is_err()
        {
            return Err(crate::error::SimulatorError::ServerError(
                "Simulator is closed".to_string(),
            ));
        }
        stream.write_all(&[STATUS_OK]).await?;
        Ok(())
    }

    async fn read_geometry(&self, stream: &mut TcpStream) -> Result<Geometry> {
        let mut num_devices_buf = [0u8; 4];
        stream.read_exact(&mut num_devices_buf).await?;
        let num_devices = u32::from_le_bytes(num_devices_buf);

        let mut devices = Vec::new();

        for _ in 0..num_devices {
            let mut pos_buf = [0u8; 12];
            stream.read_exact(&mut pos_buf).await?;
            let x = f32::from_le_bytes([pos_buf[0], pos_buf[1], pos_buf[2], pos_buf[3]]);
            let y = f32::from_le_bytes([pos_buf[4], pos_buf[5], pos_buf[6], pos_buf[7]]);
            let z = f32::from_le_bytes([pos_buf[8], pos_buf[9], pos_buf[10], pos_buf[11]]);

            let mut rot_buf = [0u8; 16];
            stream.read_exact(&mut rot_buf).await?;
            let w = f32::from_le_bytes([rot_buf[0], rot_buf[1], rot_buf[2], rot_buf[3]]);
            let i = f32::from_le_bytes([rot_buf[4], rot_buf[5], rot_buf[6], rot_buf[7]]);
            let j = f32::from_le_bytes([rot_buf[8], rot_buf[9], rot_buf[10], rot_buf[11]]);
            let k = f32::from_le_bytes([rot_buf[12], rot_buf[13], rot_buf[14], rot_buf[15]]);

            devices.push(
                autd3_driver::autd3_device::AUTD3 {
                    pos: autd3_core::geometry::Point3::new(x, y, z),
                    rot: autd3_core::geometry::UnitQuaternion { w, i, j, k },
                }
                .into(),
            );
        }

        Ok(autd3_core::geometry::Geometry::new(devices))
    }

    async fn handle_send_data(&self, stream: &mut TcpStream) -> Result<()> {
        let mut num_devices_buf = [0u8; 4];
        stream.read_exact(&mut num_devices_buf).await?;
        let num_devices = u32::from_le_bytes(num_devices_buf) as usize;

        let tx_size = std::mem::size_of::<TxMessage>();
        let mut tx_data = vec![0u8; tx_size * num_devices];
        stream.read_exact(&mut tx_data).await?;

        let tx_messages: Vec<TxMessage> = tx_data
            .chunks_exact(tx_size)
            .map(|chunk| unsafe { std::ptr::read(chunk.as_ptr() as *const TxMessage) })
            .collect();

        if self
            .proxy
            .send_event(UserEvent::Server(Signal::Send(tx_messages)))
            .is_err()
        {
            return Err(crate::error::SimulatorError::ServerError(
                "Simulator is closed".to_string(),
            ));
        }

        stream.write_all(&[STATUS_OK]).await?;
        Ok(())
    }

    async fn handle_read_data(&self, stream: &mut TcpStream) -> Result<()> {
        let rx_data = {
            let rx = self.rx_buf.read();
            let num_devices = rx.len() as u32;
            let rx_size = std::mem::size_of::<RxMessage>();
            let mut data = Vec::with_capacity(4 + rx_size * rx.len());
            data.extend_from_slice(&num_devices.to_le_bytes());
            for rx_msg in rx.iter() {
                let bytes = unsafe {
                    std::slice::from_raw_parts(rx_msg as *const RxMessage as *const u8, rx_size)
                };
                data.extend_from_slice(bytes);
            }
            data
        };

        stream.write_all(&[STATUS_OK]).await?;
        stream.write_all(&rx_data).await?;

        Ok(())
    }

    async fn handle_close(&self, stream: &mut TcpStream) -> Result<()> {
        if self
            .proxy
            .send_event(UserEvent::Server(Signal::Close))
            .is_err()
        {
            return Err(crate::error::SimulatorError::ServerError(
                "Simulator is closed".to_string(),
            ));
        }
        stream.write_all(&[STATUS_OK]).await?;
        Ok(())
    }

    pub async fn run(self, listener: TcpListener) -> Result<()> {
        loop {
            let (stream, addr) = listener.accept().await?;
            tracing::info!("New connection from: {}", addr);

            if let Err(e) = self.handle_client(stream).await {
                tracing::error!("Error handling client: {:?}", e);
            }
        }
    }
}
