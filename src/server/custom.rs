// # Protocol Specification
//
// ## Message Types
//
// - `0x01`: Configure Geometry
// - `0x02`: Update Geometry
// - `0x03`: Send Data
// - `0x04`: Read Data
// - `0x05`: Close
//
// ## Response Status Codes
//
// - `0x00`: OK
// - `0xFF`: Error
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
// Response (Success):
// - 1 byte: status (0x00 = OK)
//
// Response (Error):
// - 1 byte: status (0xFF = Error)
// - 4 bytes: error message length (u32, little-endian)
// - N bytes: error message (UTF-8 string)
//
// ### Send Data
// Request:
// - 1 byte: message type (0x03)
// - 4 bytes: number of devices (u32, little-endian)
// - Raw TxMessage data for each device
//
// Response (Success):
// - 1 byte: status (0x00 = OK)
//
// Response (Error):
// - 1 byte: status (0xFF = Error)
// - 4 bytes: error message length (u32, little-endian)
// - N bytes: error message (UTF-8 string)
//
// ### Read Data
// Request:
// - 1 byte: message type (0x04)
//
// Response (Success):
// - 1 byte: status (0x00 = OK)
// - 4 bytes: number of devices (u32, little-endian)
// - Raw RxMessage data for each device
//
// Response (Error):
// - 1 byte: status (0xFF = Error)
// - 4 bytes: error message length (u32, little-endian)
// - N bytes: error message (UTF-8 string)
//
// ### Close
// Request:
// - 1 byte: message type (0x05)
//
// Response (Success):
// - 1 byte: status (0x00 = OK)
//
// Response (Error):
// - 1 byte: status (0xFF = Error)
// - 4 bytes: error message length (u32, little-endian)
// - N bytes: error message (UTF-8 string)

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, RwLock};

use autd3_core::link::{RxMessage, TxMessage};
use autd3_driver::geometry::Geometry;
use winit::event_loop::EventLoopProxy;

use crate::error::Result;
use crate::event::{Signal, UserEvent};

const MSG_CONFIG_GEOMETRY: u8 = 0x01;
const MSG_UPDATE_GEOMETRY: u8 = 0x02;
const MSG_SEND_DATA: u8 = 0x03;
const MSG_READ_DATA: u8 = 0x04;
const MSG_CLOSE: u8 = 0x05;

const STATUS_OK: u8 = 0x00;
const STATUS_ERROR: u8 = 0xFF;

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

    fn write_error(stream: &mut TcpStream, error_msg: &str) -> std::io::Result<()> {
        stream.write_all(&[STATUS_ERROR])?;
        let msg_bytes = error_msg.as_bytes();
        let msg_len = msg_bytes.len() as u32;
        stream.write_all(&msg_len.to_le_bytes())?;
        stream.write_all(msg_bytes)?;
        Ok(())
    }

    fn handle_client(&self, mut stream: TcpStream) -> Result<()> {
        loop {
            let mut msg_type = [0u8; 1];
            if stream.read_exact(&mut msg_type).is_err() {
                break;
            }

            let result = match msg_type[0] {
                MSG_CONFIG_GEOMETRY => self.handle_config_geometry(&mut stream),
                MSG_UPDATE_GEOMETRY => self.handle_update_geometry(&mut stream),
                MSG_SEND_DATA => self.handle_send_data(&mut stream),
                MSG_READ_DATA => self.handle_read_data(&mut stream),
                MSG_CLOSE => {
                    let res = self.handle_close(&mut stream);
                    if res.is_ok() {
                        break;
                    }
                    res
                }
                _ => {
                    break;
                }
            };

            if let Err(e) = result {
                let _ = Self::write_error(&mut stream, &e.to_string());
                break;
            }
        }
        Ok(())
    }

    fn handle_config_geometry(&self, stream: &mut TcpStream) -> Result<()> {
        let geometry = self.read_geometry(stream)?;
        if self
            .proxy
            .send_event(UserEvent::Server(Signal::ConfigGeometry(geometry)))
            .is_err()
        {
            return Err(crate::error::SimulatorError::ServerError(
                "Simulator is closed".to_string(),
            ));
        }
        stream.write_all(&[STATUS_OK])?;
        Ok(())
    }

    fn handle_update_geometry(&self, stream: &mut TcpStream) -> Result<()> {
        let geometry = self.read_geometry(stream)?;
        if self
            .proxy
            .send_event(UserEvent::Server(Signal::UpdateGeometry(geometry)))
            .is_err()
        {
            return Err(crate::error::SimulatorError::ServerError(
                "Simulator is closed".to_string(),
            ));
        }
        stream.write_all(&[STATUS_OK])?;
        Ok(())
    }

    fn read_geometry(&self, stream: &mut TcpStream) -> Result<Geometry> {
        let mut num_devices_buf = [0u8; 4];
        stream.read_exact(&mut num_devices_buf)?;
        let num_devices = u32::from_le_bytes(num_devices_buf);

        let mut devices = Vec::new();

        for _ in 0..num_devices {
            let mut pos_buf = [0u8; 12];
            stream.read_exact(&mut pos_buf)?;
            let x = f32::from_le_bytes([pos_buf[0], pos_buf[1], pos_buf[2], pos_buf[3]]);
            let y = f32::from_le_bytes([pos_buf[4], pos_buf[5], pos_buf[6], pos_buf[7]]);
            let z = f32::from_le_bytes([pos_buf[8], pos_buf[9], pos_buf[10], pos_buf[11]]);

            let mut rot_buf = [0u8; 16];
            stream.read_exact(&mut rot_buf)?;
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

    fn handle_send_data(&self, stream: &mut TcpStream) -> Result<()> {
        let mut num_devices_buf = [0u8; 4];
        stream.read_exact(&mut num_devices_buf)?;
        let num_devices = u32::from_le_bytes(num_devices_buf) as usize;

        let tx_size = std::mem::size_of::<TxMessage>();
        let mut tx_data = vec![0u8; tx_size * num_devices];
        stream.read_exact(&mut tx_data)?;

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

        stream.write_all(&[STATUS_OK])?;
        Ok(())
    }

    fn handle_read_data(&self, stream: &mut TcpStream) -> Result<()> {
        let rx_data = {
            let rx = self.rx_buf.read().unwrap();
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

        stream.write_all(&[STATUS_OK])?;
        stream.write_all(&rx_data)?;

        Ok(())
    }

    fn handle_close(&self, stream: &mut TcpStream) -> Result<()> {
        if self
            .proxy
            .send_event(UserEvent::Server(Signal::Close))
            .is_err()
        {
            return Err(crate::error::SimulatorError::ServerError(
                "Simulator is closed".to_string(),
            ));
        }
        stream.write_all(&[STATUS_OK])?;
        Ok(())
    }

    pub fn run(self, listener: TcpListener) -> Result<()> {
        loop {
            let (stream, _addr) = listener.accept()?;

            let _ = self.handle_client(stream);
        }
    }
}
